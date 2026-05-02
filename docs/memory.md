# Memory System Reference

The proxy includes an optional SurrealDB-backed memory system for RAG (Retrieval-Augmented Generation). It stores documents with semantic embeddings and auto-injects relevant context into every proxied request.

---

## Building with Memory

```bash
cargo build --release --features memory
```

The default build excludes SurrealDB (~+35MB binary size increase). Use `--features memory` only when you need persistent context.

---

## Configuration

In `~/.config/oproxy/config.toml`:

```toml
[memory]
enabled = true
db_path = ""                          # defaults to ~/.local/share/oproxy/memory.db
embedding_model = "text-embedding-3-small"
```

`db_path` supports tilde expansion: `~/.local/share/oproxy/memory.db`

An OpenAI API key is required for embedding generation. The proxy reads `OPENAI_API_KEY` or `~/.codex/auth.json`.

---

## Scope Concept

Every document belongs to a **scope** — a string namespace for isolation. Common scopes:

| Scope | Purpose |
|-------|---------|
| `session` | Ephemeral per-session context (default) |
| `project` | Shared across a codebase |
| `global` | User-wide persistent knowledge |

Pass the scope in the `X-Memory-Scope` request header. Defaults to `session`.

---

## REST API

### Store a Document

```
POST /v1/memory/documents
Content-Type: application/json

{
  "scope": "project",
  "text": "The auth module uses JWT with RS256 signing.",
  "metadata": { "source": "codebase-review" }
}
```

Response: `{ "id": "<uuid>" }`

The proxy embeds the text using `text-embedding-3-small` (1536 dimensions) and stores it in SurrealDB with an HNSW vector index. Embedding calls have a 500ms timeout — documents are stored without embeddings if the call times out.

### List Documents

```
GET /v1/memory/documents?scope=project
```

Response: array of `DocumentRecord` objects.

### Delete a Document

```
DELETE /v1/memory/documents/<id>
```

Response: `204 No Content`

### Semantic Search

```
GET /v1/memory/search?q=authentication&scope=project&limit=5
```

Embeds the query, runs HNSW KNN search, returns top-k results ordered by cosine distance. Falls back to text scan if embeddings are unavailable.

---

## RAG Injection Behavior

When a chat request arrives:

1. The proxy extracts the last user message text.
2. Calls `search(user_text, scope, 3)` with a 500ms timeout.
3. If results are found, prepends a system message:

```
# Relevant Context

- The auth module uses JWT with RS256 signing.
- Token expiry is 24 hours for API keys, 1 hour for OAuth.
```

4. The enriched request is forwarded to the backend.

---

## Embedding Model Configuration

The default model is `text-embedding-3-small` (1536 dimensions, $0.02/1M tokens). To use a different model:

```toml
[memory]
embedding_model = "text-embedding-3-large"
```

The HNSW index is fixed at 1536 dimensions — changing the embedding model requires wiping the database.
