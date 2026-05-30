use crate::auth::AuthUser;
use crate::cognee::CogneeClient;
use crate::config::{AppConfig, AppEnv};
use crate::db::Db;
use crate::overview::{OverviewEventBus, OverviewRunGuard};
use crate::triggerware::TriggerwareClient;
use anyhow::{Context, Result, bail};
use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use include_dir::{Dir, include_dir};
use mime_guess::from_path;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};

static CLIENT_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/src/frontend/dist/client");

const CLIENT_ENTRY: &str = "src/frontend/entry-client.tsx";

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: Db,
    pub overview_events: OverviewEventBus,
    pub overview_runs: OverviewRunGuard,
    pub cognee: Option<Arc<CogneeClient>>,
    pub triggerware: Option<Arc<TriggerwareClient>>,
    pub watchtower: Arc<crate::watchtower::WatchtowerStatus>,
    manifest: Arc<ViteManifest>,
}

impl AppState {
    pub async fn boot(config: AppConfig) -> Result<Self> {
        let manifest = ViteManifest::load(config.env)?;
        let db = Db::connect(&config.mongodb_uri, &config.mongodb_db).await?;

        let cognee = match (&config.cognee_url, &config.cognee_email, &config.cognee_password) {
            (Some(url), Some(email), Some(password)) => {
                let client = CogneeClient::new(url, email, password);
                // Warm up auth in background so the first request is fast.
                let c2 = client.clone();
                tokio::spawn(async move {
                    if let Err(e) = c2.ensure_auth().await {
                        tracing::warn!("cognee initial auth failed: {e}");
                    } else {
                        tracing::info!("cognee ready");
                    }
                });
                Some(Arc::new(client))
            }
            _ => {
                tracing::info!("cognee not configured (COGNEE_URL/EMAIL/PASSWORD not set)");
                None
            }
        };

        let triggerware = match config.triggerware_api_key.as_deref() {
            Some(api_key) => {
                tracing::info!(base = %config.triggerware_api_url, "triggerware enabled");
                Some(Arc::new(TriggerwareClient::new(
                    &config.triggerware_api_url,
                    api_key,
                )?))
            }
            None => {
                tracing::info!("triggerware not configured (TRIGGERWARE_API_KEY not set)");
                None
            }
        };

        Ok(Self {
            config: Arc::new(config),
            db,
            overview_events: OverviewEventBus::new(),
            overview_runs: OverviewRunGuard::default(),
            cognee,
            triggerware,
            watchtower: Arc::new(crate::watchtower::WatchtowerStatus::default()),
            manifest: Arc::new(manifest),
        })
    }

    pub fn render_document(&self, page: &PagePayload) -> Result<Html<String>> {
        let serialized = serialize_for_script(page)?;
        let head_assets = self.head_assets()?;
        let footer_assets = self.footer_assets()?;
        let title = match (
            page.meta.title.trim().is_empty(),
            self.config.metadata.default_title.trim().is_empty(),
        ) {
            (false, _) => page.meta.title.as_str(),
            (true, false) => self.config.metadata.default_title.as_str(),
            (true, true) => "",
        };

        let document = format!(
            "<!DOCTYPE html>\
<html lang=\"{lang}\">\
<head>\
<meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{title}</title>\
<meta name=\"description\" content=\"{description}\">\
<link rel=\"preconnect\" href=\"https://fonts.googleapis.com\">\
<link rel=\"preconnect\" href=\"https://fonts.gstatic.com\" crossorigin>\
<link rel=\"preconnect\" href=\"https://api.fontshare.com\" crossorigin>\
<link rel=\"stylesheet\" href=\"https://fonts.googleapis.com/css2?family=Inter:wght@300..700&family=IBM+Plex+Mono:wght@400;500;600&display=swap\">\
<link rel=\"stylesheet\" href=\"https://api.fontshare.com/v2/css?f%5B%5D=chillax@200,300,400,500,600,700&display=swap\">\
{head_assets}\
</head>\
<body class=\"min-h-screen\">\
<div id=\"app\"></div>\
<script>window.dataSSr = {serialized};</script>\
{footer_assets}\
</body>\
</html>",
            lang = escape_html(&page.meta.locale),
            title = escape_html(title),
            description = escape_html(&page.meta.description),
            head_assets = head_assets,
            serialized = serialized,
            footer_assets = footer_assets,
        );

        Ok(Html(document))
    }

    pub fn read_embedded_asset(&self, path: &str) -> Option<EmbeddedAsset<'static>> {
        let clean = sanitize_relative_path(path)?;
        let file = CLIENT_DIST.get_file(clean)?;

        Some(EmbeddedAsset {
            content_type: from_path(clean).first_or_octet_stream().to_string(),
            bytes: file.contents(),
        })
    }

    fn head_assets(&self) -> Result<String> {
        if self.config.env.is_development() {
            return Ok(format!(
                "<link rel=\"stylesheet\" href=\"{origin}/src/frontend/styles.css\">",
                origin = self.config.vite_dev_server_url.trim_end_matches('/'),
            ));
        }

        let Some(entry) = self.manifest.entry(CLIENT_ENTRY) else {
            bail!("production mode requires a built Vite manifest entry for `{CLIENT_ENTRY}`");
        };

        let mut output = String::new();
        for css in &entry.css {
            output.push_str(&format!(
                "<link rel=\"stylesheet\" href=\"/{href}\">",
                href = css
            ));
        }

        Ok(output)
    }

    fn footer_assets(&self) -> Result<String> {
        if self.config.env.is_development() {
            return Ok(format!(
                "<script type=\"module\" src=\"{origin}/@vite/client\"></script>\
<script type=\"module\" src=\"{origin}/{entry}\"></script>",
                origin = self.config.vite_dev_server_url.trim_end_matches('/'),
                entry = CLIENT_ENTRY,
            ));
        }

        let Some(entry) = self.manifest.entry(CLIENT_ENTRY) else {
            bail!("production mode requires a built Vite manifest entry for `{CLIENT_ENTRY}`");
        };

        Ok(format!(
            "<script type=\"module\" src=\"/{src}\"></script>",
            src = entry.file
        ))
    }
}

pub struct EmbeddedAsset<'a> {
    pub content_type: String,
    pub bytes: &'a [u8],
}

impl<'a> EmbeddedAsset<'a> {
    pub fn into_response(self, immutable: bool) -> Response {
        let cache_control = if immutable {
            "public, max-age=31536000, immutable"
        } else {
            "public, max-age=300"
        };

        (
            [
                (
                    header::CONTENT_TYPE,
                    HeaderValue::from_str(&self.content_type)
                        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
                ),
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_static(cache_control),
                ),
            ],
            Body::from(self.bytes.to_vec()),
        )
            .into_response()
    }
}

#[derive(Debug, Serialize)]
pub struct PagePayload {
    pub request: PageRequest,
    pub meta: PageMeta,
    pub page: PageDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<AuthUser>,
}

#[derive(Debug, Serialize)]
pub struct PageRequest {
    pub path: String,
    pub status: u16,
}

#[derive(Debug, Serialize)]
pub struct PageMeta {
    pub title: String,
    pub description: String,
    pub locale: String,
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct PageDescriptor {
    pub component: &'static str,
    pub props: Value,
}

#[derive(Debug, Deserialize)]
struct ViteEntry {
    file: String,
    #[serde(default)]
    css: Vec<String>,
}

#[derive(Debug, Default)]
struct ViteManifest {
    entries: HashMap<String, ViteEntry>,
}

impl ViteManifest {
    fn load(env: AppEnv) -> Result<Self> {
        let manifest_path = ".vite/manifest.json";
        let manifest_contents = CLIENT_DIST
            .get_file(manifest_path)
            .map(|file| file.contents_utf8())
            .flatten()
            .unwrap_or("{}");

        let entries: HashMap<String, ViteEntry> = serde_json::from_str(manifest_contents)
            .with_context(|| format!("failed to parse embedded `{manifest_path}`"))?;

        if env.is_production() && !entries.contains_key(CLIENT_ENTRY) {
            bail!(
                "APP_ENV=production but the frontend bundle is missing. Run `bun run build:app` first."
            );
        }

        Ok(Self { entries })
    }

    fn entry(&self, key: &str) -> Option<&ViteEntry> {
        self.entries.get(key)
    }
}

fn sanitize_relative_path(path: &str) -> Option<&str> {
    let clean = path.trim_start_matches('/');
    if clean.is_empty() || clean.contains("..") {
        return None;
    }

    Some(clean)
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn serialize_for_script<T: Serialize>(value: &T) -> Result<String> {
    let raw = serde_json::to_string(value).context("failed to serialize page payload")?;

    Ok(raw
        .replace("</script", "<\\/script")
        .replace("<!--", "<\\!--")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}

pub fn not_found(message: &str) -> Response {
    (StatusCode::NOT_FOUND, message.to_owned()).into_response()
}
