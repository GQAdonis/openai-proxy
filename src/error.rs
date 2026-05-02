use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("upstream error {status}: {body}")]
    Upstream { status: u16, body: String },

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("http client error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("model '{model}' is not available on {profile}")]
    ModelNotAvailable { model: String, profile: String },
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, type_, message) = match &self {
            ProxyError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            ProxyError::Upstream { status, body } => {
                let code = StatusCode::from_u16(*status)
                    .unwrap_or(StatusCode::BAD_GATEWAY);
                (code, "upstream_error", body.clone())
            }
            ProxyError::Serde(e) => (StatusCode::BAD_REQUEST, "serialization_error", e.to_string()),
            ProxyError::Http(e) => (StatusCode::BAD_GATEWAY, "http_error", e.to_string()),
            ProxyError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg.clone()),
            ProxyError::ModelNotAvailable { model, profile } => (
                StatusCode::BAD_REQUEST,
                "model_not_available",
                format!("Model '{model}' is not available on {profile}"),
            ),
        };

        tracing::error!(error = %self, "proxy error");

        let body = json!({
            "error": {
                "message": message,
                "type": type_,
                "code": status.as_u16()
            }
        });

        (status, Json(body)).into_response()
    }
}
