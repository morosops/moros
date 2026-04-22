use axum::{Json, Router, routing::get};
use serde::Serialize;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse<'a> {
    pub ok: bool,
    pub service: &'a str,
}

pub fn base_router<S>(service: &'static str) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/health",
            get(move || async move { Json(HealthResponse { ok: true, service }) }),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
