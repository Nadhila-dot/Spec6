use crate::{
    auth::{self, AuthUser},
    render::{AppState, PageDescriptor, PageMeta, PagePayload, PageRequest},
};
use axum::{
    extract::State,
    http::{StatusCode, Uri},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::CookieJar;
use serde_json::json;

pub async fn document(
    State(state): State<AppState>,
    jar: CookieJar,
    uri: Uri,
) -> Result<Response, Response> {
    let user = auth::user_from_cookies(&state.db, &jar, &state.config.session_cookie)
        .await
        .map_err(internal_error)?;

    let clean_path = normalize_page_path(uri.path());
    let is_chat_path = clean_path == "/chat" || clean_path.starts_with("/chat/");

    // Gate /chat behind a session; bounce signed-in users away from auth pages.
    if is_chat_path && user.is_none() {
        return Ok(Redirect::to("/login").into_response());
    }
    if matches!(clean_path.as_str(), "/login" | "/signup") && user.is_some() {
        return Ok(Redirect::to("/chat").into_response());
    }

    let resolved = resolve_page(&state, &clean_path, user);
    let html = state
        .render_document(&resolved.payload)
        .map_err(internal_error)?;

    Ok((resolved.status, html).into_response())
}

struct ResolvedPage {
    status: StatusCode,
    payload: PagePayload,
}

fn resolve_page(state: &AppState, clean_path: &str, user: Option<AuthUser>) -> ResolvedPage {
    let metadata = &state.config.metadata;
    let url = absolute_url(state, clean_path);

    match clean_path {
        "/" => ResolvedPage {
            status: StatusCode::OK,
            payload: PagePayload {
                request: PageRequest {
                    path: clean_path.to_owned(),
                    status: StatusCode::OK.as_u16(),
                },
                meta: PageMeta {
                    title: metadata.default_title.clone(),
                    description: metadata.description.clone(),
                    locale: metadata.locale.clone(),
                    url,
                },
                page: PageDescriptor {
                    component: "landing",
                    props: json!({
                        "tagline": "Brand intelligence at the speed of code.",
                        "copy": "Sentinel is an autonomous brand threat intelligence platform. One brand name in, an analyst-grade dossier out in ninety seconds.",
                    }),
                },
                user,
            },
        },
        "/login" => ResolvedPage {
            status: StatusCode::OK,
            payload: PagePayload {
                request: PageRequest {
                    path: clean_path.to_owned(),
                    status: StatusCode::OK.as_u16(),
                },
                meta: PageMeta {
                    title: join_title("Sign in", &metadata.default_title),
                    description: metadata.description.clone(),
                    locale: metadata.locale.clone(),
                    url,
                },
                page: PageDescriptor {
                    component: "login",
                    props: json!({}),
                },
                user,
            },
        },
        "/signup" => ResolvedPage {
            status: StatusCode::OK,
            payload: PagePayload {
                request: PageRequest {
                    path: clean_path.to_owned(),
                    status: StatusCode::OK.as_u16(),
                },
                meta: PageMeta {
                    title: join_title("Create account", &metadata.default_title),
                    description: metadata.description.clone(),
                    locale: metadata.locale.clone(),
                    url,
                },
                page: PageDescriptor {
                    component: "signup",
                    props: json!({}),
                },
                user,
            },
        },
        _ if clean_path == "/chat" || clean_path.starts_with("/chat/") => {
            let conversation_id = clean_path
                .strip_prefix("/chat/")
                .map(|s| s.to_owned())
                .filter(|s| !s.is_empty());
            ResolvedPage {
                status: StatusCode::OK,
                payload: PagePayload {
                    request: PageRequest {
                        path: clean_path.to_owned(),
                        status: StatusCode::OK.as_u16(),
                    },
                    meta: PageMeta {
                        title: join_title("Chat", &metadata.default_title),
                        description: metadata.description.clone(),
                        locale: metadata.locale.clone(),
                        url,
                    },
                    page: PageDescriptor {
                        component: "chat",
                        props: json!({
                            "conversation_id": conversation_id,
                        }),
                    },
                    user,
                },
            }
        }
        _ => ResolvedPage {
            status: StatusCode::NOT_FOUND,
            payload: PagePayload {
                request: PageRequest {
                    path: clean_path.to_owned(),
                    status: StatusCode::NOT_FOUND.as_u16(),
                },
                meta: PageMeta {
                    title: join_title("Not Found", &metadata.default_title),
                    description: metadata.description.clone(),
                    locale: metadata.locale.clone(),
                    url,
                },
                page: PageDescriptor {
                    component: "not-found",
                    props: json!({
                        "path": clean_path,
                        "message": "No matching page exists for this route.",
                    }),
                },
                user,
            },
        },
    }
}

fn normalize_page_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_owned();
    }

    let without_query = trimmed
        .split_once('?')
        .map(|(value, _)| value)
        .unwrap_or(trimmed);
    let without_fragment = without_query
        .split_once('#')
        .map(|(value, _)| value)
        .unwrap_or(without_query);
    let prefixed = if without_fragment.starts_with('/') {
        without_fragment.to_owned()
    } else {
        format!("/{without_fragment}")
    };

    if prefixed.len() > 1 {
        prefixed.trim_end_matches('/').to_owned()
    } else {
        prefixed
    }
}

fn absolute_url(state: &AppState, path: &str) -> String {
    format!("{}{}", state.config.app_url.trim_end_matches('/'), path)
}

fn join_title(segment: &str, site: &str) -> String {
    let segment = segment.trim();
    let site = site.trim();

    match (segment.is_empty(), site.is_empty()) {
        (false, false) => format!("{segment} | {site}"),
        (false, true) => segment.to_owned(),
        (true, false) => site.to_owned(),
        (true, true) => String::new(),
    }
}

fn internal_error(error: anyhow::Error) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response()
}
