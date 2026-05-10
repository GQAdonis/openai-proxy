/// SurrealDB-backed memory store for RAG document storage and retrieval.
///
/// Compiled only when `--features memory` is passed. The default build has no
/// SurrealDB dependency, keeping the binary small (~8MB vs ~40MB).
use std::sync::Arc;

// ── Backend trait (always compiled) ─────────────────────────────────────────

pub trait MemoryBackend: Send + Sync {
    fn is_enabled(&self) -> bool;
}

#[derive(Default)]
pub struct NoopMemoryStore;

impl MemoryBackend for NoopMemoryStore {
    fn is_enabled(&self) -> bool { false }
}

pub type DynMemory = Arc<dyn MemoryBackend + Send + Sync>;

pub fn noop() -> DynMemory {
    Arc::new(NoopMemoryStore)
}

// ── Feature-gated concrete store + record types ─────────────────────────────

#[cfg(feature = "memory")]
pub use memory_impl::{DocumentRecord, MemoryStore, SearchResult};

#[cfg(feature = "memory")]
mod memory_impl {
    use std::{path::Path, sync::Arc, time::Duration};

    use serde::{Deserialize, Serialize};
    use surrealdb::{
        Surreal,
        engine::local::{Db, RocksDb},
        types::SurrealValue,
    };
    use tokio::time::timeout;

    use super::MemoryBackend;

    #[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
    pub struct DocumentRecord {
        pub id: String,
        pub scope: String,
        pub text: String,
        #[serde(default)]
        pub metadata: serde_json::Value,
        pub created_at: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, SurrealValue)]
    pub struct SearchResult {
        pub id: String,
        pub scope: String,
        pub text: String,
        #[serde(default)]
        pub metadata: serde_json::Value,
        pub created_at: String,
        pub distance: Option<f32>,
    }

    pub struct MemoryStore {
        pub db: Surreal<Db>,
        http_client: reqwest::Client,
        embedding_model: String,
        api_key: Option<String>,
    }

    impl MemoryBackend for MemoryStore {
        fn is_enabled(&self) -> bool { true }
    }

    impl MemoryStore {
        pub async fn open(
            path: &Path,
            http_client: reqwest::Client,
            embedding_model: String,
            api_key: Option<String>,
        ) -> anyhow::Result<Arc<Self>> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let db = Surreal::new::<RocksDb>(path.to_string_lossy().as_ref()).await?;
            db.use_ns("oproxy").use_db("memory").await?;
            Self::migrate(&db).await?;
            Ok(Arc::new(Self { db, http_client, embedding_model, api_key }))
        }

        async fn migrate(db: &Surreal<Db>) -> anyhow::Result<()> {
            db.query(
                "DEFINE TABLE IF NOT EXISTS document SCHEMAFULL;
                 DEFINE FIELD IF NOT EXISTS id           ON document TYPE string;
                 DEFINE FIELD IF NOT EXISTS scope        ON document TYPE string;
                 DEFINE FIELD IF NOT EXISTS text         ON document TYPE string;
                 DEFINE FIELD IF NOT EXISTS embedding    ON document TYPE array<float>;
                 DEFINE FIELD IF NOT EXISTS metadata     ON document TYPE object;
                 DEFINE FIELD IF NOT EXISTS created_at   ON document TYPE datetime;
                 DEFINE INDEX IF NOT EXISTS hnsw_embed   ON document FIELDS embedding
                     HNSW DIMENSION 1536 DIST COSINE TYPE F32;",
            )
            .await?;
            Ok(())
        }

        /// Embed text via OpenAI API with a 500ms timeout.
        /// Returns empty vec on failure — callers degrade gracefully.
        pub async fn embed_text(&self, text: &str) -> Vec<f32> {
            let Some(ref api_key) = self.api_key else { return Vec::new() };
            let body = serde_json::json!({ "model": self.embedding_model, "input": text });
            let fut = self.http_client
                .post("https://api.openai.com/v1/embeddings")
                .bearer_auth(api_key)
                .json(&body)
                .send();

            let resp = match timeout(Duration::from_millis(500), fut).await {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => { tracing::warn!(error = %e, "embedding request failed"); return Vec::new(); }
                Err(_) => { tracing::warn!("embedding request timed out"); return Vec::new(); }
            };

            let json: serde_json::Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => { tracing::warn!(error = %e, "embedding response parse failed"); return Vec::new(); }
            };

            json["data"][0]["embedding"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
                .unwrap_or_default()
        }

        pub async fn store_document(
            &self,
            scope: &str,
            text: &str,
            metadata: serde_json::Value,
        ) -> anyhow::Result<String> {
            let id = uuid::Uuid::new_v4().to_string();
            let id_for_query = id.clone();
            let scope_owned = scope.to_string();
            let text_owned = text.to_string();
            let metadata_owned = metadata.clone();
            let now = chrono::Utc::now().to_rfc3339();
            let embedding = self.embed_text(text).await;
            self.db
                .query("CREATE type::thing('document', $id) SET id = $id, scope = $scope, text = $text, embedding = $embedding, metadata = $metadata, created_at = <datetime>$now")
                .bind(("id", id_for_query))
                .bind(("scope", scope_owned))
                .bind(("text", text_owned))
                .bind(("embedding", embedding))
                .bind(("metadata", metadata_owned))
                .bind(("now", now))
                .await?;
            Ok(id)
        }

        pub async fn delete_document(&self, id: &str) -> anyhow::Result<()> {
            let id_owned = id.to_string();
            self.db
                .query("DELETE type::thing('document', $id)")
                .bind(("id", id_owned))
                .await?;
            Ok(())
        }

        pub async fn list_documents(&self, scope: &str) -> anyhow::Result<Vec<DocumentRecord>> {
            let scope_owned = scope.to_string();
            let mut resp = self.db
                .query("SELECT id, scope, text, metadata, time::format(created_at, '%Y-%m-%dT%H:%M:%SZ') AS created_at FROM document WHERE scope = $scope")
                .bind(("scope", scope_owned))
                .await?;
            let docs: Vec<DocumentRecord> = resp.take(0)?;
            Ok(docs)
        }

        pub async fn search_documents(
            &self,
            query: &str,
            scope: &str,
            limit: usize,
        ) -> anyhow::Result<Vec<SearchResult>> {
            let embedding = self.embed_text(query).await;
            if embedding.is_empty() {
                let docs = self.list_documents(scope).await?;
                return Ok(docs.into_iter().take(limit).map(|d| SearchResult {
                    id: d.id, scope: d.scope, text: d.text,
                    metadata: d.metadata, created_at: d.created_at, distance: None,
                }).collect());
            }

            let scope_owned = scope.to_string();
            let mut resp = self.db
                .query(format!(
                    "SELECT id, scope, text, metadata, \
                     time::format(created_at, '%Y-%m-%dT%H:%M:%SZ') AS created_at, \
                     vector::distance::knn() AS distance \
                     FROM document WHERE scope = $scope \
                     AND embedding <|{limit},40|> $query_vec \
                     ORDER BY distance LIMIT {limit}"
                ))
                .bind(("scope", scope_owned))
                .bind(("query_vec", embedding))
                .await?;
            let results: Vec<SearchResult> = resp.take(0)?;
            Ok(results)
        }

        /// Shorthand for RAG injection — returns `DocumentRecord`s.
        pub async fn search(
            &self,
            query: &str,
            scope: &str,
            limit: usize,
        ) -> anyhow::Result<Vec<DocumentRecord>> {
            let results = self.search_documents(query, scope, limit).await?;
            Ok(results.into_iter().map(|r| DocumentRecord {
                id: r.id, scope: r.scope, text: r.text,
                metadata: r.metadata, created_at: r.created_at,
            }).collect())
        }
    }
}

// ── REST handlers ─────────────────────────────────────────────────────────────

#[cfg(feature = "memory")]
pub mod handlers {
    use axum::{
        Json,
        extract::{Path, Query, State},
        http::StatusCode,
    };
    use serde::Deserialize;

    use crate::AppState;
    use super::memory_impl::{DocumentRecord, SearchResult};

    #[derive(Deserialize)]
    pub struct CreateDocumentBody {
        pub scope: String,
        pub text: String,
        #[serde(default)]
        pub metadata: serde_json::Value,
    }

    #[derive(Deserialize)]
    pub struct ScopeQuery {
        pub scope: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct SearchQuery {
        pub q: String,
        pub scope: Option<String>,
        #[serde(default = "default_limit")]
        pub limit: usize,
    }

    fn default_limit() -> usize { 5 }

    pub async fn create_document(
        State(state): State<AppState>,
        Json(body): Json<CreateDocumentBody>,
    ) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
        let store = state.memory_store
            .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "memory not enabled".to_string()))?;
        let id = store.store_document(&body.scope, &body.text, body.metadata)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Json(serde_json::json!({ "id": id })))
    }

    pub async fn delete_document(
        State(state): State<AppState>,
        Path(id): Path<String>,
    ) -> Result<StatusCode, (StatusCode, String)> {
        let store = state.memory_store
            .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "memory not enabled".to_string()))?;
        store.delete_document(&id)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(StatusCode::NO_CONTENT)
    }

    pub async fn list_documents(
        State(state): State<AppState>,
        Query(params): Query<ScopeQuery>,
    ) -> Result<Json<Vec<DocumentRecord>>, (StatusCode, String)> {
        let store = state.memory_store
            .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "memory not enabled".to_string()))?;
        let scope = params.scope.as_deref().unwrap_or("session");
        let docs = store.list_documents(scope)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Json(docs))
    }

    pub async fn search_documents(
        State(state): State<AppState>,
        Query(params): Query<SearchQuery>,
    ) -> Result<Json<Vec<SearchResult>>, (StatusCode, String)> {
        let store = state.memory_store
            .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "memory not enabled".to_string()))?;
        let scope = params.scope.as_deref().unwrap_or("session");
        let results = store.search_documents(&params.q, scope, params.limit)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Json(results))
    }
}