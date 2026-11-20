use axum::{
    Router,
    body::Body,
    extract::Path,
    http::{HeaderMap, Method, Response, StatusCode, header},
    response::{IntoResponse, Redirect},
    routing::get,
};

include!(concat!(env!("OUT_DIR"), "/web_assets.rs"));

pub fn router() -> Router {
    Router::new()
        .route("/ui", get(|| async { Redirect::permanent("/ui/") }))
        .route(
            "/ui/",
            get(|method: Method, headers: HeaderMap| async move { serve("", method, &headers) }),
        )
        .route(
            "/ui/{*path}",
            get(
                |Path(path): Path<String>, method: Method, headers: HeaderMap| async move {
                    serve(&path, method, &headers)
                },
            ),
        )
}

fn serve(path: &str, method: Method, headers: &HeaderMap) -> Response<Body> {
    let html = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',').any(|item| {
                let mut parts = item.trim().split(';');
                parts.next() == Some("text/html")
                    && !parts.any(|p| {
                        p.trim()
                            .strip_prefix("q=")
                            .and_then(|q| q.parse::<f32>().ok())
                            == Some(0.0)
                    })
            })
        });
    let lookup = match path {
        "" | "index.html" | "settings" | "settings/" if html => "index.html",
        "favicon.svg" => path,
        _ if path.starts_with("assets/") => path,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let Ok(index) = ASSETS.binary_search_by_key(&lookup, |(key, _, _)| *key) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (_, mime, bytes) = ASSETS[index];
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, bytes.len())
        .header(header::CACHE_CONTROL, if lookup.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" })
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "no-referrer")
        .header("content-security-policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; connect-src 'self'; img-src 'self' data:; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'")
        .body(if method == Method::HEAD { Body::empty() } else { Body::from(bytes) })
        .expect("valid embedded asset headers")
}
