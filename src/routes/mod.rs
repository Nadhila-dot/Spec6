mod api;
mod app;
mod public;

use crate::render::AppState;
use axum::{Router, routing::get};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/up", get(up))
        .merge(public::router())
        .merge(api::router())
        .route("/", get(app::document))
        .route("/*path", get(app::document))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn up() -> &'static str {
    "ok"
}
