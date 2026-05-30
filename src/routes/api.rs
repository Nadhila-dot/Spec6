use crate::{
    agent,
    auth::{self, AuthError, AuthUser},
    chat,
    cognee,
    config::InferenceProvider,
    inference::{
        self, ChatRole, ChatTurn, InferenceModelSummary, InferenceSelection, InferenceStreamEvent,
    },
    overview,
    prompt,
    render::AppState,
    speechmatics,
    watchtower,
};
use axum::{
    Json, Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, patch, post},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use chrono::Utc;
use futures::{SinkExt, StreamExt, future::BoxFuture};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    env,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use time::Duration;
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;

static SSE_EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn spawn_cognee_chat_memory_ingest(
    state: Arc<AppState>,
    group_id: Option<String>,
    conversation_id: &str,
    user_body: &str,
    assistant_body: &str,
) {
    let Some(cognee) = state.cognee.clone() else {
        return;
    };
    let Some(group_id) = group_id else {
        return;
    };

    let dataset = cognee::dataset_name_for_group(&group_id);
    let timestamp = Utc::now();
    let conversation_id = conversation_id.to_owned();
    let memory_text = format!(
        "Chat memory entry\nTimestamp UTC: {}\nConversation id: {}\n\nUser message:\n{}\n\nAssistant answer:\n{}",
        timestamp.to_rfc3339(),
        conversation_id,
        user_body.trim(),
        assistant_body.trim(),
    );
    let filename = format!("chat-memory-{}-{}", conversation_id, timestamp.format("%Y%m%d-%H%M%S"));
    tokio::spawn(async move {
        if let Err(err) = cognee
            .ingest_and_cognify(&dataset, &memory_text, &filename)
            .await
        {
            tracing::warn!("cognee chat memory ingest failed for conversation {conversation_id}: {err}");
        } else {
            tracing::info!("cognee: ingested chat memory for conversation {conversation_id}");
        }
    });
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/signup", post(signup))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/me", get(me))
        .route("/api/inference/catalog", get(inference_catalog))
        .route(
            "/api/chat-groups",
            get(list_chat_groups).post(create_chat_group),
        )
        .route(
            "/api/chat-groups/:id",
            patch(update_chat_group).delete(delete_chat_group_handler),
        )
        .route("/api/chat-groups/:id/overview", get(get_chat_group_overview))
        .route(
            "/api/chat-groups/:id/overview/stream",
            get(stream_chat_group_overview),
        )
        .route("/api/chat-groups/:id/triggers", get(list_chat_group_triggers))
        .route(
            "/api/chat-groups/:id/triggers/sync",
            post(sync_chat_group_triggers),
        )
        .route(
            "/api/chat-groups/:id/cognee/search",
            post(cognee_search_handler),
        )
        .route(
            "/api/chat-groups/:id/cognee/status",
            get(cognee_status_handler),
        )
        .route(
            "/api/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/api/conversations/:id",
            get(get_conversation)
                .patch(rename_conversation)
                .delete(delete_conversation),
        )
        .route(
            "/api/conversations/:id/group",
            patch(set_conversation_group),
        )
        .route("/api/conversations/:id/messages", post(send_message))
        .route("/api/conversations/:id/messages/ws", get(send_message_ws))
        .route("/api/voice/status", get(voice_status))
        .route("/api/voice/tts", post(voice_tts))
        .route("/api/voice/transcribe/ws", get(voice_transcribe_ws))
        .route("/api/watchtower/status", get(watchtower_status))
        .route("/api/watchtower/run", post(watchtower_run))
}

async fn voice_status(State(state): State<AppState>, jar: CookieJar) -> Response {
    if let Err(resp) = require_user(&state, &jar).await {
        return resp;
    }
    Json(json!({
        "enabled": runtime_speechmatics_api_key(&state).is_some(),
        "provider": "speechmatics",
    }))
    .into_response()
}

async fn voice_tts(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<VoiceTtsBody>,
) -> Response {
    if let Err(resp) = require_user(&state, &jar).await {
        return resp;
    }

    let Some(api_key) = runtime_speechmatics_api_key(&state) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "speech output unavailable" })),
        )
            .into_response();
    };

    let raw_text = body.text.trim();
    if raw_text.is_empty() {
        return bad_request("text is required");
    }

    let text = if body.full.unwrap_or(false) {
        raw_text.chars().take(4_000).collect::<String>()
    } else {
        raw_text.chars().take(900).collect::<String>()
    };

    let audio = match speechmatics::synthesize_speech(
        &api_key,
        &runtime_speechmatics_tts_url(&state),
        &runtime_speechmatics_tts_voice(&state),
        &text,
        "wav_16000",
    )
    .await
    {
        Ok(audio) => audio,
        Err(err) => {
            tracing::warn!(error = ?err, "speechmatics tts failed");
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "speech output failed" })),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "audio/wav"),
            (
                header::CACHE_CONTROL,
                "no-store, no-cache, must-revalidate, max-age=0",
            ),
        ],
        audio,
    )
        .into_response()
}

async fn voice_transcribe_ws(
    State(state): State<AppState>,
    jar: CookieJar,
    ws: WebSocketUpgrade,
) -> Response {
    if let Err(resp) = require_user(&state, &jar).await {
        return resp;
    }
    ws.on_upgrade(move |socket| handle_voice_socket(socket, state))
        .into_response()
}

async fn watchtower_status(State(state): State<AppState>, jar: CookieJar) -> Response {
    if let Err(resp) = require_user(&state, &jar).await {
        return resp;
    }
    Json(state.watchtower.snapshot()).into_response()
}

async fn watchtower_run(State(state): State<AppState>, jar: CookieJar) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    // Manual patrols ignore staleness and only touch the caller's companies.
    match watchtower::patrol(&state, chrono::Duration::zero(), Some(user_oid)).await {
        Ok(triggered) => Json(json!({ "scans_triggered": triggered })).into_response(),
        Err(err) => internal(err),
    }
}

#[derive(Debug, Deserialize)]
struct SignupBody {
    username: String,
    display_name: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct SendMessageBody {
    body: String,
    provider: Option<String>,
    model: Option<String>,
    response_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VoiceTtsBody {
    text: String,
    full: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CreateConversationBody {
    group_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatGroupBody {
    name: String,
    data_text: String,
}

#[derive(Debug, Deserialize)]
struct SetConversationGroupBody {
    group_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RenameBody {
    title: String,
}

async fn signup(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<SignupBody>,
) -> Response {
    match auth::signup(
        &state.db,
        &body.username,
        &body.display_name,
        &body.password,
    )
    .await
    {
        Ok((user, token)) => {
            let jar = jar.add(session_cookie(&state, token));
            (jar, Json(json!({ "user": user }))).into_response()
        }
        Err(err) => auth_error_response(err),
    }
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<LoginBody>,
) -> Response {
    match auth::login(&state.db, &body.username, &body.password).await {
        Ok((user, token)) => {
            let jar = jar.add(session_cookie(&state, token));
            (jar, Json(json!({ "user": user }))).into_response()
        }
        Err(err) => auth_error_response(err),
    }
}

async fn logout(State(state): State<AppState>, jar: CookieJar) -> Response {
    if let Some(cookie) = jar.get(&state.config.session_cookie) {
        let _ = auth::logout(&state.db, cookie.value()).await;
    }
    let jar = jar.remove(Cookie::from(state.config.session_cookie.clone()));
    (jar, StatusCode::NO_CONTENT).into_response()
}

async fn me(State(state): State<AppState>, jar: CookieJar) -> Response {
    match auth::user_from_cookies(&state.db, &jar, &state.config.session_cookie).await {
        Ok(Some(user)) => Json(json!({ "user": user })).into_response(),
        Ok(None) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "not signed in" })),
        )
            .into_response(),
        Err(err) => internal(err),
    }
}

async fn list_conversations(State(state): State<AppState>, jar: CookieJar) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    match chat::list_conversations(&state.db, oid).await {
        Ok(items) => Json(json!({ "conversations": items })).into_response(),
        Err(err) => internal(err),
    }
}

async fn inference_catalog(State(state): State<AppState>, jar: CookieJar) -> Response {
    if let Err(resp) = require_user(&state, &jar).await {
        return resp;
    }

    let mut providers = Vec::new();
    for provider in state.config.available_inference_providers() {
        match inference::list_models(&state.config, provider).await {
            Ok(models) => {
                let default_model = pick_default_model(&state.config, provider, &models);
                providers.push(InferenceCatalogProvider {
                    id: provider.as_str().to_owned(),
                    label: provider.label().to_owned(),
                    available: !models.is_empty(),
                    default_model,
                    models,
                    error: None,
                });
            }
            Err(err) => {
                providers.push(InferenceCatalogProvider {
                    id: provider.as_str().to_owned(),
                    label: provider.label().to_owned(),
                    available: false,
                    default_model: None,
                    models: Vec::new(),
                    error: Some(err.to_string()),
                });
            }
        }
    }

    let default_provider = state
        .config
        .default_inference_provider()
        .map(|provider| provider.as_str().to_owned())
        .filter(|provider_id| {
            providers
                .iter()
                .any(|provider| provider.available && provider.id == *provider_id)
        })
        .or_else(|| {
            providers
                .iter()
                .find(|provider| provider.available)
                .map(|provider| provider.id.clone())
        });

    Json(InferenceCatalogResponse {
        default_provider,
        providers,
    })
    .into_response()
}

async fn list_chat_groups(State(state): State<AppState>, jar: CookieJar) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    match chat::list_chat_groups(&state.db, oid).await {
        Ok(groups) => Json(json!({ "groups": groups })).into_response(),
        Err(err) => internal(err),
    }
}

async fn create_chat_group(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<ChatGroupBody>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    match chat::create_chat_group(&state.db, oid, &body.name, &body.data_text).await {
        Ok(group) => {
            let group_id = group.id.clone();
            let group_oid = match ObjectId::parse_str(&group_id) {
                Ok(value) => value,
                Err(_) => return internal(anyhow::anyhow!("inserted chat group id invalid")),
            };
            if overview::should_queue_company_overview(&group.name, &group.data_text) {
                overview::queue_company_overview(state.clone(), oid, group_oid).await;
            }
            let state_clone = state.clone();
            let group_clone = group.clone();
            tokio::spawn(async move {
                if let Err(err) = crate::triggerware::sync_company_monitors(
                    &state_clone,
                    oid,
                    group_oid,
                    &group_clone.name,
                    &group_clone.data_text,
                )
                .await
                {
                    tracing::warn!(error = ?err, group_id = %group_id, "triggerware sync failed after company create");
                }
            });
            Json(json!({ "group": group })).into_response()
        }
        Err(err) => bad_request(&err.to_string()),
    }
}

async fn update_chat_group(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<ChatGroupBody>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let group_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid chat group id"),
    };
    match chat::update_chat_group(&state.db, user_oid, group_oid, &body.name, &body.data_text).await
    {
        Ok(Some(group)) => {
            let target_company_name = overview::company_name_for_group(&group.name, &group.data_text);
            if overview::should_queue_company_overview(&group.name, &group.data_text) {
                // Skip if an overview is already running or has already completed for this company.
                // Failed runs are allowed to retry on the next save.
                let existing = overview::load_company_overview(&state.db, user_oid, group_oid)
                    .await
                    .ok()
                    .flatten();
                let existing_status = existing.as_ref().map(|item| item.status.clone());
                let stale_company_name = existing
                    .as_ref()
                    .map(|item| !item.company_name.trim().eq_ignore_ascii_case(target_company_name.trim()))
                    .unwrap_or(false);
                let skip = matches!(
                    existing_status,
                    Some(overview::CompanyOverviewStatus::Queued)
                        | Some(overview::CompanyOverviewStatus::Running)
                        | Some(overview::CompanyOverviewStatus::Completed)
                ) && !stale_company_name;
                if !skip {
                    if stale_company_name
                        && matches!(
                            existing_status,
                            Some(overview::CompanyOverviewStatus::Queued)
                                | Some(overview::CompanyOverviewStatus::Running)
                        )
                    {
                        overview::queue_company_overview_after_current(
                            state.clone(),
                            user_oid,
                            group_oid,
                        );
                    } else {
                    overview::queue_company_overview(state.clone(), user_oid, group_oid).await;
                    }
                }
            }

            // Sync company profile into Cognee knowledge graph (fire-and-forget).
            if let Some(cognee) = state.cognee.clone() {
                let dataset = cognee::dataset_name_for_group(&id);
                let company_text = format!(
                    "Company: {}\n\n{}",
                    group.name,
                    group.data_text
                );
                let fname = format!("company-profile-{id}");
                let id_for_cognee = id.clone();
                tokio::spawn(async move {
                    if let Err(e) = cognee.ingest_and_cognify(&dataset, &company_text, &fname).await {
                        tracing::warn!("cognee ingest failed for group {id_for_cognee}: {e}");
                    } else {
                        tracing::info!("cognee: ingested company profile for group {id_for_cognee}");
                    }
                });
            }

            let state_clone = state.clone();
            let group_clone = group.clone();
            let id_for_triggerware = id.clone();
            tokio::spawn(async move {
                if let Err(err) = crate::triggerware::sync_company_monitors(
                    &state_clone,
                    user_oid,
                    group_oid,
                    &group_clone.name,
                    &group_clone.data_text,
                )
                .await
                {
                    tracing::warn!(error = ?err, group_id = %id_for_triggerware, "triggerware sync failed after company update");
                }
            });

            Json(json!({ "group": group })).into_response()
        }
        Ok(None) => not_found("chat group not found"),
        Err(err) => bad_request(&err.to_string()),
    }
}

async fn delete_chat_group_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let group_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid chat group id"),
    };
    match chat::delete_chat_group(&state.db, user_oid, group_oid).await {
        Ok(true) => {
            let _ = overview::delete_company_overview(&state.db, user_oid, group_oid).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => not_found("chat group not found"),
        Err(err) => internal(err),
    }
}

async fn list_chat_group_triggers(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let group_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid chat group id"),
    };
    match crate::triggerware::list_company_trigger_events(&state.db, user_oid, group_oid).await {
        Ok(events) => Json(json!({ "events": events })).into_response(),
        Err(err) => internal(err),
    }
}

async fn sync_chat_group_triggers(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let group_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid chat group id"),
    };
    let Some(group) = (match chat::load_chat_group(&state.db, user_oid, group_oid).await {
        Ok(group) => group,
        Err(err) => return internal(err),
    }) else {
        return not_found("chat group not found");
    };
    match crate::triggerware::sync_and_poll_company_monitors(
        &state,
        user_oid,
        group_oid,
        &group.name,
        &group.data_text,
    )
    .await
    {
        Ok(summary) => Json(json!({ "summary": summary })).into_response(),
        Err(err) => internal(err),
    }
}

async fn get_chat_group_overview(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let group_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid chat group id"),
    };

    match chat::load_chat_group(&state.db, user_oid, group_oid).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("chat group not found"),
        Err(err) => return internal(err),
    }

    match overview::load_company_overview(&state.db, user_oid, group_oid).await {
        Ok(result) => Json(json!({ "overview": result })).into_response(),
        Err(err) => internal(err),
    }
}

async fn stream_chat_group_overview(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let group_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid chat group id"),
    };

    match chat::load_chat_group(&state.db, user_oid, group_oid).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found("chat group not found"),
        Err(err) => return internal(err),
    }

    let group_id = id.clone();
    let mut rx = state.overview_events.subscribe();
    let (tx, stream_rx) = mpsc::channel::<Result<Event, Infallible>>(32);
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(envelope) => {
                    if envelope.company_id != group_id {
                        continue;
                    }
                    let _ = tx
                        .send(Ok(Event::default().event(&envelope.event).data(
                            serde_json::to_string(&envelope.payload).unwrap_or_else(|_| {
                                "{\"error\":\"serialization failed\"}".to_owned()
                            }),
                        )))
                        .await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    });

    Sse::new(ReceiverStream::new(stream_rx))
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(10)))
        .into_response()
}

async fn create_conversation(
    State(state): State<AppState>,
    jar: CookieJar,
    body: Option<Json<CreateConversationBody>>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let group_oid = match resolve_group_id_for_user(
        &state,
        oid,
        body.as_ref()
            .and_then(|Json(body)| body.group_id.as_deref()),
    )
    .await
    {
        Ok(group_oid) => group_oid,
        Err(resp) => return resp,
    };
    match chat::create_conversation(&state.db, oid, group_oid).await {
        Ok(convo) => Json(json!({ "conversation": convo })).into_response(),
        Err(err) => internal(err),
    }
}

async fn get_conversation(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let convo_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid conversation id"),
    };

    match chat::load_conversation(&state.db, user_oid, convo_oid).await {
        Ok(Some((conversation, messages))) => {
            Json(json!({ "conversation": conversation, "messages": messages })).into_response()
        }
        Ok(None) => not_found("conversation not found"),
        Err(err) => internal(err),
    }
}

async fn rename_conversation(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<RenameBody>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let convo_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid conversation id"),
    };
    match chat::rename_conversation(&state.db, user_oid, convo_oid, &body.title).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found("conversation not found"),
        Err(err) => bad_request(&err.to_string()),
    }
}

async fn delete_conversation(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let convo_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid conversation id"),
    };
    match chat::delete_conversation(&state.db, user_oid, convo_oid).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found("conversation not found"),
        Err(err) => internal(err),
    }
}

async fn set_conversation_group(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<SetConversationGroupBody>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let convo_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid conversation id"),
    };
    let group_oid =
        match resolve_group_id_for_user(&state, user_oid, body.group_id.as_deref()).await {
            Ok(group_oid) => group_oid,
            Err(resp) => return resp,
        };

    match chat::set_conversation_group(&state.db, user_oid, convo_oid, group_oid).await {
        Ok(Some(conversation)) => Json(json!({ "conversation": conversation })).into_response(),
        Ok(None) => not_found("conversation not found"),
        Err(err) => internal(err),
    }
}

async fn send_message(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<SendMessageBody>,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let convo_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid conversation id"),
    };

    let (conversation, history) =
        match chat::load_conversation(&state.db, user_oid, convo_oid).await {
            Ok(Some(existing)) => existing,
            Ok(None) => return not_found("conversation not found"),
            Err(err) => return internal(err),
        };
    let trimmed = match chat::validate_message_body(&body.body) {
        Ok(value) => value,
        Err(err) => return bad_request(&err.to_string()),
    };
    let runtime_context =
        match overview::chat_runtime_context(&state, user_oid, &conversation, &trimmed).await {
            Ok(value) => value,
            Err(err) => return internal(err),
        };
    let conversation_group_id = conversation.group_id.clone();
    let group_data_text =
        prompt_context_for_mode(runtime_context.prompt_context, body.response_mode.as_deref());
    let selection = match resolve_inference_selection(
        &state.config,
        body.provider.as_deref(),
        body.model.as_deref(),
    ) {
        Ok(selection) => selection,
        Err(err) => return bad_request(&err.to_string()),
    };

    let user_msg = match chat::append_message(&state.db, convo_oid, "user", &trimmed).await {
        Ok(msg) => msg,
        Err(err) => return internal(err),
    };

    let turns = build_inference_turns(&history, &trimmed);
    let state = Arc::new(state);
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);
    let user_message_for_title = trimmed.clone();
    let selection_for_stream = selection.clone();
    let live_title = Arc::new(Mutex::new(None::<String>));

    tokio::spawn(async move {
        let mut assistant_body = String::new();
        let live_title_for_stream = Arc::clone(&live_title);
        if !runtime_context.tool_calls.is_empty() {
            let _ = tx
                .send(Ok(sse_event(
                    "tool",
                    &ToolEvent {
                        calls: runtime_context.tool_calls.clone(),
                    },
                )))
                .await;
        }
        let stream_result = inference::stream_text(
            &state.config,
            &selection_for_stream,
            &turns,
            group_data_text.as_deref(),
            |event| -> BoxFuture<'static, anyhow::Result<()>> {
                match event {
                    InferenceStreamEvent::TextDelta(delta) => {
                        assistant_body.push_str(&delta);
                        let tx = tx.clone();
                        Box::pin(async move {
                            let _ = tx.send(Ok(sse_event("token", &TokenEvent { delta }))).await;
                            Ok(())
                        })
                    }
                    InferenceStreamEvent::MetaTitle(title) => {
                        let tx = tx.clone();
                        let state = Arc::clone(&state);
                        let live_title = Arc::clone(&live_title_for_stream);
                        Box::pin(async move {
                            let mut guard = live_title.lock().await;
                            if guard.is_none() {
                                let saved =
                                    chat::set_generated_title_if_new(&state.db, convo_oid, &title)
                                        .await
                                        .ok()
                                        .flatten();

                                if let Some(saved_title) = saved {
                                    *guard = Some(saved_title.clone());
                                    let _ = tx
                                        .send(Ok(sse_event(
                                            "meta",
                                            &MetaEvent { title: saved_title },
                                        )))
                                        .await;
                                }
                            }
                            Ok(())
                        })
                    }
                }
            },
        )
        .await;

        match stream_result {
            Ok(()) => {
                if assistant_body.trim().is_empty() {
                    let _ = tx
                        .send(Ok(sse_event(
                            "error",
                            &ErrorEvent {
                                error: "assistant returned an empty response".to_owned(),
                            },
                        )))
                        .await;
                    return;
                }

                let assistant_msg =
                    match chat::append_message(&state.db, convo_oid, "assistant", &assistant_body)
                        .await
                    {
                        Ok(msg) => msg,
                        Err(err) => {
                            let _ = tx
                                .send(Ok(sse_event(
                                    "error",
                                    &ErrorEvent {
                                        error: format!("failed to save assistant reply: {err}"),
                                    },
                                )))
                                .await;
                            return;
                        }
                    };

                spawn_cognee_chat_memory_ingest(
                    Arc::clone(&state),
                    conversation_group_id.clone(),
                    &convo_oid.to_hex(),
                    &trimmed,
                    &assistant_body,
                );

                let mut title_update = live_title.lock().await.clone();
                if title_update.is_none() {
                    if let Some(title) =
                        prompt::fallback_title_from_topic(&user_message_for_title, &assistant_body)
                    {
                        title_update =
                            chat::set_generated_title_if_new(&state.db, convo_oid, &title)
                                .await
                                .ok()
                                .flatten();

                        if let Some(saved_title) = &title_update {
                            let mut guard = live_title.lock().await;
                            *guard = Some(saved_title.clone());
                            let _ = tx
                                .send(Ok(sse_event(
                                    "meta",
                                    &MetaEvent {
                                        title: saved_title.clone(),
                                    },
                                )))
                                .await;
                        }
                    }
                }

                let _ = tx
                    .send(Ok(sse_event(
                        "done",
                        &DoneEvent {
                            user: user_msg,
                            assistant: assistant_msg,
                            title: title_update,
                        },
                    )))
                    .await;
            }
            Err(err) => {
                let _ = tx
                    .send(Ok(sse_event(
                        "error",
                        &ErrorEvent {
                            error: err.to_string(),
                        },
                    )))
                    .await;
            }
        }
    });

    Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(10)))
        .into_response()
}

async fn send_message_ws(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    let user = match require_user(&state, &jar).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    let user_oid = match auth::require_object_id(&user) {
        Ok(oid) => oid,
        Err(err) => return internal(err),
    };
    let convo_oid = match ObjectId::parse_str(&id) {
        Ok(oid) => oid,
        Err(_) => return bad_request("invalid conversation id"),
    };

    ws.on_upgrade(move |socket| handle_message_socket(socket, state, user_oid, convo_oid))
        .into_response()
}

async fn handle_message_socket(
    mut socket: WebSocket,
    state: AppState,
    user_oid: ObjectId,
    convo_oid: ObjectId,
) {
    let request = match socket.recv().await {
        Some(Ok(Message::Text(text))) => match serde_json::from_str::<SendMessageBody>(&text) {
            Ok(body) => body,
            Err(err) => {
                send_socket_error(&mut socket, format!("invalid websocket request: {err}")).await;
                return;
            }
        },
        Some(Ok(Message::Close(_))) | None => return,
        Some(Ok(_)) => {
            send_socket_error(&mut socket, "first websocket frame must be JSON text").await;
            return;
        }
        Some(Err(err)) => {
            tracing::debug!(error = ?err, "chat websocket initial receive failed");
            return;
        }
    };

    let (conversation, history) =
        match chat::load_conversation(&state.db, user_oid, convo_oid).await {
            Ok(Some(existing)) => existing,
            Ok(None) => {
                send_socket_error(&mut socket, "conversation not found").await;
                return;
            }
            Err(err) => {
                tracing::error!(error = ?err, "failed to load websocket conversation");
                send_socket_error(&mut socket, "internal error").await;
                return;
            }
        };
    let trimmed = match chat::validate_message_body(&request.body) {
        Ok(value) => value,
        Err(err) => {
            send_socket_error(&mut socket, err.to_string()).await;
            return;
        }
    };
    let runtime_context =
        match overview::chat_runtime_context(&state, user_oid, &conversation, &trimmed).await {
            Ok(value) => value,
            Err(err) => {
                tracing::error!(error = ?err, "failed to build websocket chat runtime context");
                send_socket_error(&mut socket, "internal error").await;
                return;
            }
        };
    let group_data_text =
        prompt_context_for_mode(runtime_context.prompt_context, request.response_mode.as_deref());
    let selection = match resolve_inference_selection(
        &state.config,
        request.provider.as_deref(),
        request.model.as_deref(),
    ) {
        Ok(selection) => selection,
        Err(err) => {
            send_socket_error(&mut socket, err.to_string()).await;
            return;
        }
    };

    let user_msg = match chat::append_message(&state.db, convo_oid, "user", &trimmed).await {
        Ok(msg) => msg,
        Err(err) => {
            tracing::error!(error = ?err, "failed to save websocket user message");
            send_socket_error(&mut socket, "internal error").await;
            return;
        }
    };

    let _ = runtime_context.tool_calls; // legacy pre-flight tool list is unused now
    let conversation_group_id = conversation.group_id.clone();
    let initial_turns = build_inference_turns(&history, &trimmed);
    let state = Arc::new(state);
    let (tx, mut rx) = mpsc::channel::<String>(32);
    let user_message_for_title = trimmed.clone();
    let selection_for_stream = selection.clone();

    let stream_task = tokio::spawn(async move {
        let agent_outcome = run_agent_loop(
            Arc::clone(&state),
            convo_oid,
            selection_for_stream,
            group_data_text,
            conversation_group_id.clone(),
            initial_turns,
            tx.clone(),
        )
        .await;

        let assistant_body = match agent_outcome {
            Ok(body) if !body.trim().is_empty() => body,
            Ok(_) => {
                let _ = tx
                    .send(ws_event(
                        "error",
                        &ErrorEvent {
                            error: "assistant returned an empty response".to_owned(),
                        },
                    ))
                    .await;
                return;
            }
            Err(err) => {
                let _ = tx
                    .send(ws_event(
                        "error",
                        &ErrorEvent {
                            error: err.to_string(),
                        },
                    ))
                    .await;
                return;
            }
        };

        // If the model forgot to emit a <spec6-pins> block, fire a focused
        // extraction call so the canvas map still gets analyst-grade tooltips.
        // Failure is silent — we save the original body untouched.
        let assistant_body = ensure_pin_block(
            &state.config,
            &selection,
            &user_message_for_title,
            &assistant_body,
        )
        .await;

        let assistant_msg =
            match chat::append_message(&state.db, convo_oid, "assistant", &assistant_body).await {
                Ok(msg) => msg,
                Err(err) => {
                    let _ = tx
                        .send(ws_event(
                            "error",
                            &ErrorEvent {
                                error: format!("failed to save assistant reply: {err}"),
                            },
                        ))
                        .await;
                    return;
                }
            };

        spawn_cognee_chat_memory_ingest(
            Arc::clone(&state),
            conversation_group_id.clone(),
            &convo_oid.to_hex(),
            &trimmed,
            &assistant_body,
        );

        let title_update = if let Some(title) =
            prompt::fallback_title_from_topic(&user_message_for_title, &assistant_body)
        {
            let saved = chat::set_generated_title_if_new(&state.db, convo_oid, &title)
                .await
                .ok()
                .flatten();
            if let Some(saved_title) = &saved {
                let _ = tx
                    .send(ws_event(
                        "meta",
                        &MetaEvent {
                            title: saved_title.clone(),
                        },
                    ))
                    .await;
            }
            saved
        } else {
            None
        };

        let _ = tx
            .send(ws_event(
                "done",
                &DoneEvent {
                    user: user_msg,
                    assistant: assistant_msg,
                    title: title_update,
                },
            ))
            .await;
    });

    while let Some(frame) = rx.recv().await {
        if socket.send(Message::Text(frame)).await.is_err() {
            stream_task.abort();
            return;
        }
    }

    let _ = stream_task.await;
    let _ = socket.close().await;
}

async fn handle_voice_socket(mut socket: WebSocket, state: AppState) {
    let Some(api_key) = runtime_speechmatics_api_key(&state) else {
        let _ = socket
            .send(Message::Text(
                json!({
                    "type": "error",
                    "error": "Realtime voice is unavailable right now."
                })
                .to_string(),
            ))
            .await;
        let _ = socket.close().await;
        return;
    };

    let temp_key = match speechmatics::fetch_temporary_key(
        &api_key,
        &runtime_speechmatics_mgmt_url(&state),
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(error = ?err, "speechmatics temp key failed");
            let _ = socket
                .send(Message::Text(
                    json!({
                        "type": "error",
                        "error": "Couldn't initialize realtime voice."
                    })
                    .to_string(),
                ))
                .await;
            let _ = socket.close().await;
            return;
        }
    };

    let mut upstream = match speechmatics::connect_realtime(
        &runtime_speechmatics_rt_url(&state),
        &temp_key,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(error = ?err, "speechmatics realtime connect failed");
            let _ = socket
                .send(Message::Text(
                    json!({
                        "type": "error",
                        "error": "Couldn't connect to realtime voice."
                    })
                    .to_string(),
                ))
                .await;
            let _ = socket.close().await;
            return;
        }
    };

    if upstream
        .send(tokio_tungstenite::tungstenite::Message::Text(
            speechmatics::start_recognition_message().into(),
        ))
        .await
        .is_err()
    {
        let _ = socket
            .send(Message::Text(
                json!({
                    "type": "error",
                    "error": "Couldn't start recognition."
                })
                .to_string(),
            ))
            .await;
        let _ = socket.close().await;
        return;
    }

    let _ = socket
        .send(Message::Text(json!({ "type": "ready" }).to_string()))
        .await;

    let mut seq_no: u64 = 0;
    let mut eos_sent = false;

    loop {
        tokio::select! {
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Binary(bytes))) => {
                        seq_no = seq_no.saturating_add(1);
                        if upstream
                            .send(tokio_tungstenite::tungstenite::Message::Binary(bytes.into()))
                            .await
                            .is_err()
                        {
                            let _ = socket.send(Message::Text(json!({
                                "type": "error",
                                "error": "Realtime voice stream failed."
                            }).to_string())).await;
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if text.contains("\"stop\"") && !eos_sent {
                            eos_sent = true;
                            let _ = upstream
                                .send(tokio_tungstenite::tungstenite::Message::Text(
                                    speechmatics::end_of_stream_message(seq_no).into(),
                                ))
                                .await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        tracing::debug!(error = ?err, "voice socket receive failed");
                        break;
                    }
                }
            }
            upstream_msg = upstream.next() => {
                match upstream_msg {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        match speechmatics::parse_server_message(&text) {
                            speechmatics::SpeechmaticsEvent::Started => {}
                            speechmatics::SpeechmaticsEvent::Partial(transcript) => {
                                let _ = socket.send(Message::Text(json!({
                                    "type": "partial",
                                    "transcript": transcript,
                                }).to_string())).await;
                            }
                            speechmatics::SpeechmaticsEvent::Final(transcript) => {
                                let _ = socket.send(Message::Text(json!({
                                    "type": "final",
                                    "transcript": transcript,
                                }).to_string())).await;
                            }
                            speechmatics::SpeechmaticsEvent::EndOfTranscript => {
                                let _ = socket.send(Message::Text(json!({
                                    "type": "end"
                                }).to_string())).await;
                                break;
                            }
                            speechmatics::SpeechmaticsEvent::Error(error) => {
                                let _ = socket.send(Message::Text(json!({
                                    "type": "error",
                                    "error": error,
                                }).to_string())).await;
                                break;
                            }
                            speechmatics::SpeechmaticsEvent::Other => {}
                        }
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | None => {
                        let _ = socket.send(Message::Text(json!({
                            "type": "end"
                        }).to_string())).await;
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        tracing::debug!(error = ?err, "speechmatics upstream receive failed");
                        let _ = socket.send(Message::Text(json!({
                            "type": "error",
                            "error": "Realtime voice stream ended unexpectedly."
                        }).to_string())).await;
                        break;
                    }
                }
            }
        }
    }

    let _ = upstream.close(None).await;
    let _ = socket.close().await;
}

fn runtime_speechmatics_api_key(state: &AppState) -> Option<String> {
    state
        .config
        .speechmatics_api_key
        .clone()
        .or_else(|| env::var("SPEECHMATICS_API_KEY").ok())
        .filter(|value| !value.trim().is_empty())
}

fn runtime_speechmatics_rt_url(state: &AppState) -> String {
    env::var("SPEECHMATICS_RT_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| state.config.speechmatics_rt_url.clone())
}

fn runtime_speechmatics_mgmt_url(state: &AppState) -> String {
    env::var("SPEECHMATICS_MGMT_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| state.config.speechmatics_mgmt_url.clone())
}

fn runtime_speechmatics_tts_url(state: &AppState) -> String {
    env::var("SPEECHMATICS_TTS_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| state.config.speechmatics_tts_url.clone())
}

fn runtime_speechmatics_tts_voice(state: &AppState) -> String {
    env::var("SPEECHMATICS_TTS_VOICE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| state.config.speechmatics_tts_voice.clone())
}

fn prompt_context_for_mode(
    prompt_context: Option<String>,
    response_mode: Option<&str>,
) -> Option<String> {
    if !matches!(response_mode.map(|value| value.trim().to_ascii_lowercase()), Some(mode) if mode == "voice") {
        return prompt_context;
    }

    let voice_block = prompt::voice_assistant_style_block();
    match prompt_context {
        Some(existing) if !existing.trim().is_empty() => Some(format!("{existing}\n\n{voice_block}")),
        _ => Some(voice_block),
    }
}

/// Drive the LLM ↔ tool loop. Streams `tool_started` / `tool_completed` events
/// over the WS channel as tools run, and streams the FINAL assistant pass to
/// the client as token deltas. Intermediate "thinking" passes (which contain
/// tool tags) are buffered and not forwarded as visible text.
async fn run_agent_loop(
    state: Arc<AppState>,
    convo_oid: ObjectId,
    selection: InferenceSelection,
    group_data_text: Option<String>,
    group_id: Option<String>,
    initial_turns: Vec<ChatTurn>,
    tx: mpsc::Sender<String>,
) -> anyhow::Result<String> {
    let mut turns = initial_turns;
    let _ = convo_oid; // reserved for future per-iteration logging

    // Deterministic pre-dispatch. Open-source models tend to skip the
    // specialised marketplace scouts even when the prompt lists them; for
    // high-signal intents we seed the first iteration ourselves so the UI
    // lights up the right persona row and the model sees the right evidence.
    let seed_invocations = collect_seed_invocations(&state, &turns, &group_id).await;
    if !seed_invocations.is_empty() {
        let seed_count = seed_invocations.len();
        let seed_outcomes = dispatch_seed_invocations(&state, &group_id, &seed_invocations, &tx).await;
        let mut seed_paired = Vec::with_capacity(seed_count);
        for (inv, outcome) in seed_invocations.into_iter().zip(seed_outcomes.into_iter()) {
            seed_paired.push((inv, outcome));
        }
        // Slip the synthetic exchange into history so the upcoming LLM pass
        // sees the evidence already on the table. We attribute the call to a
        // helper "router" so the model knows it didn't fire these itself.
        turns.push(agent::assistant_turn_from_pass(
            "(Spec6 router auto-dispatched marketplace + research scouts based on the user request.)",
        ));
        turns.push(agent::user_tool_results_turn(agent::render_tool_results_turn(
            &seed_paired,
        )));
    }

    for iteration in 0..agent::MAX_AGENT_ITERATIONS {
        let pass = agent::collect_pass(
            &state.config,
            &selection,
            &turns,
            group_data_text.as_deref(),
        )
        .await?;

        let base_id = format!("call-{}-{}", iteration, agent::now_unix_ms());
        let (visible_text, invocations) = agent::parse_tool_calls(&pass, &base_id);

        if invocations.is_empty() {
            // Final pass — stream the visible text to the client.
            let final_text = complete_cutoff_answer(
                &state.config,
                &selection,
                turns.clone(),
                group_data_text.as_deref(),
                visible_text,
            )
            .await?;
            stream_final_text(&tx, &final_text).await;
            return Ok(final_text);
        }

        // Cap how many tools we'll run per turn.
        let invocations: Vec<_> = invocations
            .into_iter()
            .take(agent::MAX_TOOLS_PER_TURN)
            .collect();

        // Emit started events
        for inv in &invocations {
            let started = agent::now_unix_ms();
            let _ = tx
                .send(ws_event(
                    "tool_started",
                    &agent::ToolStartedEvent {
                        id: inv.id.clone(),
                        name: inv.name.clone(),
                        source_type: inv.source_type.clone(),
                        query: inv.query.clone(),
                        label: agent::tool_label(&inv.source_type, &inv.query),
                        iteration,
                        started_at_unix_ms: started,
                    },
                ))
                .await;
        }

        // Run all tools in parallel (SERP + Cognee)
        let company_context = match group_id.as_deref() {
            Some(id) => fetch_group_context(&state, id).await,
            None => None,
        };

        let outcomes = agent::run_tools(
            &state.config,
            &invocations,
            state.cognee.clone(),
            company_context.as_ref(),
        )
        .await;

        // Emit completed events + collect for the next user turn
        let mut paired = Vec::with_capacity(invocations.len());
        for (inv, outcome) in invocations.into_iter().zip(outcomes.into_iter()) {
            let status = if outcome.error.is_some() {
                "failed"
            } else {
                "completed"
            };
            let _ = tx
                .send(ws_event(
                    "tool_completed",
                    &agent::ToolCompletedEvent {
                        id: inv.id.clone(),
                        source_type: outcome.source_type.clone(),
                        query: outcome.query.clone(),
                        result_count: outcome.result_count(),
                        elapsed_ms: outcome.elapsed_ms,
                        status: status.to_owned(),
                        error: outcome.error.clone(),
                    },
                ))
                .await;
            paired.push((inv, outcome));
        }

        // Continue the conversation: append the assistant's pass (with tool
        // tags intact) and the synthetic user turn carrying the results.
        turns.push(agent::assistant_turn_from_pass(&pass));
        turns.push(agent::user_tool_results_turn(
            agent::render_tool_results_turn(&paired),
        ));
    }

    // Loop exhausted — force a final, no-tools answer.
    turns.push(ChatTurn {
        role: ChatRole::User,
        body: "Tool budget exhausted. Write the final answer now using whatever evidence you already have. Do NOT emit any <tool …/> tags.".to_owned(),
    });
    let final_pass = agent::collect_pass(
        &state.config,
        &selection,
        &turns,
        group_data_text.as_deref(),
    )
    .await?;
    let (visible_text, _ignored) = agent::parse_tool_calls(&final_pass, "final");
    let final_text = complete_cutoff_answer(
        &state.config,
        &selection,
        turns,
        group_data_text.as_deref(),
        visible_text,
    )
    .await?;
    stream_final_text(&tx, &final_text).await;
    Ok(final_text)
}

async fn complete_cutoff_answer(
    config: &crate::config::AppConfig,
    selection: &InferenceSelection,
    mut turns: Vec<ChatTurn>,
    group_data_text: Option<&str>,
    mut answer: String,
) -> anyhow::Result<String> {
    const MAX_CONTINUATIONS: usize = 2;

    for _ in 0..MAX_CONTINUATIONS {
        if !answer_looks_cut_off(&answer) {
            break;
        }

        turns.push(ChatTurn {
            role: ChatRole::Assistant,
            body: answer.clone(),
        });
        turns.push(ChatTurn {
            role: ChatRole::User,
            body: "Your last answer stopped mid-sentence or mid-markdown. Continue exactly from the next character. Do not restart, summarize, add tool calls, or repeat existing text. Finish the current table/list/section cleanly.".to_owned(),
        });

        let continuation =
            agent::collect_pass(config, selection, &turns, group_data_text).await?;
        let (visible, _ignored) = agent::parse_tool_calls(&continuation, "continuation");
        let visible = visible.trim_start_matches(['\r', '\n']);
        if visible.trim().is_empty() {
            break;
        }
        answer.push_str(visible);
    }

    Ok(answer)
}

fn answer_looks_cut_off(answer: &str) -> bool {
    let trimmed = answer.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    if has_unclosed_markdown_fence(trimmed) || has_unclosed_custom_block(trimmed) {
        return true;
    }

    let last_line = trimmed.lines().last().unwrap_or(trimmed).trim();
    if last_line.is_empty() {
        return false;
    }

    if last_line.starts_with('|') && !last_line.ends_with('|') {
        return true;
    }
    if last_line.starts_with('-') && last_line.len() < 18 {
        return true;
    }

    let hard_end = ['.', '!', '?', ')', ']', '}', '`'];
    if hard_end.iter().any(|ch| trimmed.ends_with(*ch)) {
        return false;
    }

    let lower = last_line.to_ascii_lowercase();
    let dangling_tail = [
        " and", " or", " but", " with", " for", " from", " to", " the", " a", " an",
        " of", " in", " on", " at", " by", " because", " while", " where", " which",
    ];
    dangling_tail.iter().any(|tail| lower.ends_with(tail))
        || last_line.ends_with(',')
        || last_line.ends_with(':')
        || last_line.ends_with(';')
        || last_line.ends_with("**")
        || last_line.chars().count() < 24
}

fn has_unclosed_markdown_fence(text: &str) -> bool {
    text.matches("```").count() % 2 == 1
}

fn has_unclosed_custom_block(text: &str) -> bool {
    for (open, close) in [
        ("<spec6-pins>", "</spec6-pins>"),
        ("<spec6-trend>", "</spec6-trend>"),
        ("<think>", "</think>"),
        ("<thinking>", "</thinking>"),
    ] {
        if text.contains(open) && !text.contains(close) {
            return true;
        }
    }
    false
}

/// Inspect the latest user turn + group context and return any deterministic
/// tool invocations the router should fire before the LLM runs.
async fn collect_seed_invocations(
    state: &Arc<AppState>,
    turns: &[ChatTurn],
    group_id: &Option<String>,
) -> Vec<agent::ToolInvocation> {
    let last_user = turns
        .iter()
        .rev()
        .find(|t| matches!(t.role, ChatRole::User))
        .map(|t| t.body.clone())
        .unwrap_or_default();
    if last_user.trim().is_empty() {
        return Vec::new();
    }

    let company_context = match group_id.as_deref() {
        Some(id) => fetch_group_context(state, id).await,
        None => None,
    };

    let base_id = format!("router-{}", agent::now_unix_ms());
    agent::seed_invocations_for_user_message(
        &last_user,
        company_context.as_ref().and_then(|ctx| ctx.company.as_ref()),
        &base_id,
    )
}

async fn fetch_group_context(
    state: &Arc<AppState>,
    group_hex: &str,
) -> Option<agent::AgentCompanyContext> {
    let oid = ObjectId::parse_str(group_hex).ok()?;
    use mongodb::bson::doc;
    let doc = state
        .db
        .chat_groups()
        .find_one(doc! { "_id": oid }, None)
        .await
        .ok()
        .flatten()?;
    Some(agent::AgentCompanyContext {
        group_id: doc.id.map(|id| id.to_hex()),
        company: Some(overview::make_chat_company_context(&doc.name, &doc.data_text)),
    })
}

/// Run the seeded tools in parallel, emitting tool_started/tool_completed
/// events over the WS channel as they fire. Mirrors the per-iteration loop.
async fn dispatch_seed_invocations(
    state: &Arc<AppState>,
    group_id: &Option<String>,
    invocations: &[agent::ToolInvocation],
    tx: &mpsc::Sender<String>,
) -> Vec<crate::overview::ChatToolOutcome> {
    for inv in invocations {
        let started = agent::now_unix_ms();
        let _ = tx
            .send(ws_event(
                "tool_started",
                &agent::ToolStartedEvent {
                    id: inv.id.clone(),
                    name: inv.name.clone(),
                    source_type: inv.source_type.clone(),
                    query: inv.query.clone(),
                    label: agent::tool_label(&inv.source_type, &inv.query),
                    iteration: 0,
                    started_at_unix_ms: started,
                },
            ))
            .await;
    }

    let company_context = match group_id.as_deref() {
        Some(id) => fetch_group_context(state, id).await,
        None => None,
    };

    let outcomes = agent::run_tools(
        &state.config,
        invocations,
        state.cognee.clone(),
        company_context.as_ref(),
    )
    .await;

    for (inv, outcome) in invocations.iter().zip(outcomes.iter()) {
        let status = if outcome.error.is_some() {
            "failed"
        } else {
            "completed"
        };
        let _ = tx
            .send(ws_event(
                "tool_completed",
                &agent::ToolCompletedEvent {
                    id: inv.id.clone(),
                    source_type: outcome.source_type.clone(),
                    query: outcome.query.clone(),
                    result_count: outcome.result_count(),
                    elapsed_ms: outcome.elapsed_ms,
                    status: status.to_owned(),
                    error: outcome.error.clone(),
                },
            ))
            .await;
    }
    outcomes
}

/// Make sure the final assistant answer carries a <spec6-pins> block.
/// If the model forgot to emit one, fire a focused extraction call against
/// the same provider/model and splice the result into the answer body.
///
/// Returns the (possibly augmented) body. Network failures are non-fatal —
/// the original body is returned unchanged.
async fn ensure_pin_block(
    config: &crate::config::AppConfig,
    selection: &InferenceSelection,
    user_message: &str,
    answer: &str,
) -> String {
    let lower_user = user_message.to_lowercase();
    let map_requested = agent::is_map_request(&lower_user);
    let has_pins = answer.contains("<spec6-pins>");

    // Fire when:
    //   • pins are missing, OR
    //   • the user explicitly asked for a map (force re-emit even if a stub
    //     block exists, since the model often emits a 1-pin block as a
    //     concession when explicitly asked to plot).
    if has_pins && !map_requested {
        return answer.to_owned();
    }
    if user_message.trim().is_empty() || answer.trim().is_empty() {
        return answer.to_owned();
    }

    let system_prompt = prompt::pin_extraction_system_prompt();
    let directive = if map_requested {
        "The user explicitly asked for the answer plotted on the map. Emit 5–8 specific pins with $ figures or % share. Output the <spec6-pins> block NOW and nothing else."
    } else {
        "Emit the <spec6-pins> block now and nothing else."
    };
    let user_prompt = format!(
        "User question:\n{user}\n\nAnalyst answer:\n{ans}\n\n{directive}",
        user = user_message.trim(),
        ans = answer.trim(),
    );

    match tokio::time::timeout(
        std::time::Duration::from_secs(20),
        inference::generate_text(config, selection, &system_prompt, &user_prompt),
    )
    .await
    {
        Ok(Ok(reply)) => {
            if let Some(block) = extract_pin_block(&reply) {
                // If the user asked for a map AND the original answer already
                // had a (likely token-starved) pin block, replace it.
                let base = if has_pins && map_requested {
                    answer
                        .split("<spec6-pins>")
                        .next()
                        .unwrap_or(answer)
                        .trim_end()
                        .to_owned()
                } else {
                    answer.trim_end().to_owned()
                };
                return format!("{base}\n\n{block}");
            }
            tracing::debug!("pin extraction produced no parseable block");
            answer.to_owned()
        }
        Ok(Err(err)) => {
            tracing::debug!(error = ?err, "pin extraction call failed");
            answer.to_owned()
        }
        Err(_) => {
            tracing::debug!("pin extraction call timed out");
            answer.to_owned()
        }
    }
}

fn extract_pin_block(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let open = lower.find("<spec6-pins>")?;
    let close_rel = lower[open..].find("</spec6-pins>")?;
    let end = open + close_rel + "</spec6-pins>".len();
    Some(text[open..end].to_owned())
}

/// Chunk the final answer into bite-size deltas so the UI gets a typewriter
/// effect even though we buffered the pass internally.
async fn stream_final_text(tx: &mpsc::Sender<String>, text: &str) {
    const CHUNK_CHARS: usize = 36;
    let chars: Vec<char> = text.chars().collect();
    let mut idx = 0;
    while idx < chars.len() {
        let end = (idx + CHUNK_CHARS).min(chars.len());
        let delta: String = chars[idx..end].iter().collect();
        let _ = tx
            .send(ws_event("token", &TokenEvent { delta }))
            .await;
        idx = end;
        // Small breath so React/flushSync can paint each chunk.
        tokio::time::sleep(std::time::Duration::from_millis(8)).await;
    }
}

async fn send_socket_error(socket: &mut WebSocket, error: impl Into<String>) {
    let _ = socket
        .send(Message::Text(ws_event(
            "error",
            &ErrorEvent {
                error: error.into(),
            },
        )))
        .await;
}

#[derive(Debug, Serialize)]
struct TokenEvent {
    delta: String,
}

#[derive(Debug, Serialize)]
struct MetaEvent {
    title: String,
}

#[derive(Debug, Serialize)]
struct InferenceCatalogResponse {
    default_provider: Option<String>,
    providers: Vec<InferenceCatalogProvider>,
}

#[derive(Debug, Serialize)]
struct InferenceCatalogProvider {
    id: String,
    label: String,
    available: bool,
    default_model: Option<String>,
    models: Vec<InferenceModelSummary>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct DoneEvent {
    user: chat::Message,
    assistant: chat::Message,
    title: Option<String>,
}

#[derive(Debug, Serialize)]
struct ErrorEvent {
    error: String,
}

#[derive(Debug, Serialize)]
struct ToolEvent {
    calls: Vec<overview::ChatToolCall>,
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<AuthUser, Response> {
    match auth::user_from_cookies(&state.db, jar, &state.config.session_cookie).await {
        Ok(Some(user)) => Ok(user),
        Ok(None) => Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "not signed in" })),
        )
            .into_response()),
        Err(err) => Err(internal(err)),
    }
}

async fn resolve_group_id_for_user(
    state: &AppState,
    user_oid: ObjectId,
    raw_group_id: Option<&str>,
) -> Result<Option<ObjectId>, Response> {
    let Some(raw_group_id) = raw_group_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let group_oid =
        ObjectId::parse_str(raw_group_id).map_err(|_| bad_request("invalid chat group id"))?;
    match chat::load_chat_group(&state.db, user_oid, group_oid).await {
        Ok(Some(_)) => Ok(Some(group_oid)),
        Ok(None) => Err(not_found("chat group not found")),
        Err(err) => Err(internal(err)),
    }
}

pub fn session_cookie(state: &AppState, token: String) -> Cookie<'static> {
    let max_age = Duration::days(auth::session_days());
    let mut cookie = Cookie::new(state.config.session_cookie.clone(), token);
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    cookie.set_path("/");
    cookie.set_max_age(max_age);
    cookie.set_secure(state.config.env.is_production());
    cookie
}

/* ─── Cognee handlers ────────────────────────────────────────────────────── */

#[derive(Debug, Deserialize)]
struct CogneeSearchBody {
    query: String,
    #[serde(default = "default_search_type")]
    search_type: String,
}

fn default_search_type() -> String {
    "GRAPH_COMPLETION".to_owned()
}

async fn cognee_search_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(body): Json<CogneeSearchBody>,
) -> Response {
    if let Err(resp) = require_user(&state, &jar).await {
        return resp;
    }
    let cognee = match &state.cognee {
        Some(c) => c.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "Cognee not configured" })),
            )
                .into_response();
        }
    };
    let dataset = cognee::dataset_name_for_group(&id);
    match cognee.search(&body.query, &dataset, &body.search_type).await {
        Ok(results) => Json(json!({ "results": results })).into_response(),
        Err(err) => {
            tracing::warn!("cognee search error: {err}");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": err.to_string() })),
            )
                .into_response()
        }
    }
}

async fn cognee_status_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Response {
    if let Err(resp) = require_user(&state, &jar).await {
        return resp;
    }
    let enabled = state.cognee.is_some();
    Json(json!({ "enabled": enabled })).into_response()
}

fn auth_error_response(err: AuthError) -> Response {
    let (status, message) = match &err {
        AuthError::UsernameTaken => (StatusCode::CONFLICT, err.to_string()),
        AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, err.to_string()),
        AuthError::BadInput(_) => (StatusCode::BAD_REQUEST, err.to_string()),
        AuthError::Internal(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal error".to_string(),
        ),
    };
    if let AuthError::Internal(e) = &err {
        tracing::error!(error = ?e, "auth internal error");
    }
    (status, Json(json!({ "error": message }))).into_response()
}

fn internal(err: anyhow::Error) -> Response {
    tracing::error!(error = ?err, "internal server error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "internal error" })),
    )
        .into_response()
}

fn bad_request(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
}

fn not_found(message: &str) -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": message }))).into_response()
}

fn build_inference_turns(history: &[chat::Message], new_user_message: &str) -> Vec<ChatTurn> {
    let mut turns = Vec::with_capacity(history.len() + 1);
    for message in history {
        let role = match message.role.as_str() {
            "user" => ChatRole::User,
            "assistant" => ChatRole::Assistant,
            _ => continue,
        };

        if message.body.trim().is_empty() {
            continue;
        }

        turns.push(ChatTurn {
            role,
            body: message.body.clone(),
        });
    }

    turns.push(ChatTurn {
        role: ChatRole::User,
        body: new_user_message.to_owned(),
    });

    turns
}

fn resolve_inference_selection(
    config: &crate::config::AppConfig,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<InferenceSelection, anyhow::Error> {
    let provider = match provider {
        Some(value) if !value.trim().is_empty() => InferenceProvider::try_from(value)?,
        _ => config
            .default_inference_provider()
            .ok_or_else(|| anyhow::anyhow!("no inference providers are configured"))?,
    };

    if !config.inference_provider_enabled(provider) {
        return Err(anyhow::anyhow!(
            "{} is not configured for inference",
            provider.label()
        ));
    }

    let model = model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| config.configured_or_default_model(provider));

    Ok(InferenceSelection { provider, model })
}

fn pick_default_model(
    config: &crate::config::AppConfig,
    provider: InferenceProvider,
    models: &[InferenceModelSummary],
) -> Option<String> {
    if models.is_empty() {
        return None;
    }

    let configured = config.configured_or_default_model(provider);
    if models.iter().any(|model| model.id == configured) {
        Some(configured)
    } else {
        models.first().map(|model| model.id.clone())
    }
}

fn sse_event<T: Serialize>(event: &str, payload: &T) -> Event {
    let (event_id, value) = stream_event_payload(payload);
    Event::default().event(event).id(event_id.to_string()).data(
        serde_json::to_string(&value)
            .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_owned()),
    )
}

fn ws_event<T: Serialize>(event: &str, payload: &T) -> String {
    let (_event_id, mut value) = stream_event_payload(payload);
    if let Value::Object(map) = &mut value {
        map.insert("type".to_owned(), Value::String(event.to_owned()));
        map.insert(
            "server_transport".to_owned(),
            Value::String("websocket".to_owned()),
        );
    } else {
        value = json!({
            "type": event,
            "server_transport": "websocket",
            "data": value,
        });
    }
    serde_json::to_string(&value)
        .unwrap_or_else(|_| "{\"type\":\"error\",\"error\":\"serialization failed\"}".to_owned())
}

fn stream_event_payload<T: Serialize>(payload: &T) -> (u64, Value) {
    let event_id = SSE_EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let sent_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let mut value = serde_json::to_value(payload)
        .unwrap_or_else(|_| json!({ "error": "serialization failed" }));
    let timing_fields = [
        (
            "server_event_id",
            Value::Number(serde_json::Number::from(event_id)),
        ),
        (
            "server_sent_at_unix_ns",
            Value::String(sent_at.as_nanos().to_string()),
        ),
        (
            "server_sent_at_unix_ms",
            Value::String(sent_at.as_millis().to_string()),
        ),
    ];

    if let Value::Object(map) = &mut value {
        for (key, field_value) in timing_fields {
            map.insert(key.to_owned(), field_value);
        }
    } else {
        value = json!({
            "data": value,
            "server_event_id": event_id,
            "server_sent_at_unix_ns": sent_at.as_nanos().to_string(),
            "server_sent_at_unix_ms": sent_at.as_millis().to_string(),
        });
    }

    (event_id, value)
}
