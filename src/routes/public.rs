use crate::render::{AppState, not_found};
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/assets/*path", get(asset))
        .route("/favicon.ico", get(root_file))
        .route("/robots.txt", get(root_file))
        // Data files shipped in src/frontend/public/.
        .route("/world-110m.json", get(root_file))
}

async fn asset(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Response, Response> {
    if state.config.env.is_development() {
        let target = format!(
            "{}/assets/{}",
            state.config.vite_dev_server_url.trim_end_matches('/'),
            path
        );

        return Ok(Redirect::temporary(&target).into_response());
    }

    let disk_path = format!("assets/{path}");
    let Some(asset) = state.read_embedded_asset(&disk_path) else {
        return Err(not_found("asset not found"));
    };

    Ok(asset.into_response(true))
}

async fn root_file(
    State(state): State<AppState>,
    uri: axum::http::Uri,
) -> Result<Response, Response> {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return Err((StatusCode::NOT_FOUND, "file not found").into_response());
    }

    if state.config.env.is_development() {
        let target = format!(
            "{}/{}",
            state.config.vite_dev_server_url.trim_end_matches('/'),
            path
        );

        return Ok(Redirect::temporary(&target).into_response());
    }

    let Some(asset) = state.read_embedded_asset(path) else {
        return Err(not_found("file not found"));
    };

    Ok(asset.into_response(false))
}
