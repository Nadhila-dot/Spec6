use crate::{
    cognee::{self, CogneeClient},
    config::AppConfig,
    db::{Db, TriggerEventDoc},
    overview,
    render::AppState,
};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures::stream::TryStreamExt;
use mongodb::bson::{Document, doc, oid::ObjectId};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const DEFAULT_TRIGGER_INTERVAL_SECS: u64 = 900;
const TRIGGERWARE_START_DELAY_SECS: u64 = 45;
const OVERVIEW_REFRESH_MINUTES_AFTER_DELTA: i64 = 30;
const MAX_ROW_PREVIEW: usize = 3;
static TRIGGERWARE_NO_CONNECTORS_WARNED: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
pub struct TriggerwareClient {
    http: Client,
    base_url: String,
    api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerRecord {
    pub name: String,
    pub query: String,
    pub schedule: Option<String>,
    pub status: Option<String>,
    pub delivery: Option<Value>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledConnector {
    pub name: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub config_set: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TriggerDelta {
    #[serde(default)]
    pub added: Vec<Vec<Value>>,
    #[serde(default)]
    pub deleted: Vec<Vec<Value>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TriggerEventView {
    pub trigger_name: String,
    pub trigger_kind: String,
    pub title: String,
    pub body: String,
    pub severity: String,
    pub delivered_channels: Vec<String>,
    pub sources: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct TriggerSyncSummary {
    pub synced_triggers: usize,
    pub fired_events: usize,
}

#[derive(Debug, Clone)]
struct CompanyMonitorSpec {
    kind: &'static str,
    trigger_name: String,
    query: String,
}

impl TriggerwareClient {
    pub fn new(base_url: &str, api_key: &str) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("failed to build triggerware client")?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key: api_key.to_owned(),
        })
    }

    fn with_api_key(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        builder.header("Api-Key", &self.api_key)
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let response = self
            .with_api_key(self.http.get(format!(
                "{}/{}",
                self.base_url,
                path.trim_start_matches('/')
            )))
            .send()
            .await
            .context("triggerware GET failed")?;
        parse_response(response).await
    }

    async fn post<B: Serialize, T: for<'de> Deserialize<'de>>(&self, path: &str, body: &B) -> Result<T> {
        let response = self
            .with_api_key(
                self.http
                    .post(format!("{}/{}", self.base_url, path.trim_start_matches('/')))
                    .json(body),
            )
            .send()
            .await
            .context("triggerware POST failed")?;
        parse_response(response).await
    }

    async fn patch<B: Serialize, T: for<'de> Deserialize<'de>>(&self, path: &str, body: &B) -> Result<T> {
        let response = self
            .with_api_key(
                self.http
                    .patch(format!("{}/{}", self.base_url, path.trim_start_matches('/')))
                    .json(body),
            )
            .send()
            .await
            .context("triggerware PATCH failed")?;
        parse_response(response).await
    }

    pub async fn list_triggers(&self) -> Result<Vec<TriggerRecord>> {
        self.get("triggers").await
    }

    pub async fn list_installed_connectors(&self) -> Result<Vec<InstalledConnector>> {
        self.get("connectors/installed").await
    }

    pub async fn query_english(&self, query: &str) -> Result<Value> {
        self.post(
            "query",
            &json!({
                "query": query,
                "language": "english",
            }),
        )
        .await
    }

    pub async fn upsert_trigger(&self, name: &str, query: &str, schedule_secs: u64) -> Result<()> {
        let body = json!({
            "name": name,
            "prompt": query,
            "language": "english",
            "schedule": schedule_secs,
            "status": "enabled",
            "delivery": {
                "managed_by": "spec6",
                "channels": ["slack", "discord"]
            }
        });

        let create_response = self
            .with_api_key(self.http.post(format!("{}/triggers", self.base_url)).json(&body))
            .send()
            .await
            .context("triggerware trigger create failed")?;

        if create_response.status().is_success() {
            return Ok(());
        }

        if matches!(
            create_response.status(),
            StatusCode::CONFLICT | StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
        ) {
            let encoded = urlencoding::encode(name);
            let _: Value = self.patch(&format!("triggers/{encoded}"), &body).await?;
            return Ok(());
        }

        let status = create_response.status();
        let body = create_response.text().await.unwrap_or_default();
        Err(anyhow!("triggerware trigger create failed {status}: {body}"))
    }

    pub async fn poll_trigger(&self, name: &str) -> Result<TriggerDelta> {
        let encoded = urlencoding::encode(name);
        self.post(&format!("triggers/{encoded}/poll"), &json!({})).await
    }
}

pub fn spawn(state: AppState) {
    if state.triggerware.is_none() {
        tracing::info!("triggerware monitor disabled (TRIGGERWARE_API_KEY not set)");
        return;
    }

    let interval_secs = state
        .config
        .triggerware_poll_interval_secs
        .max(DEFAULT_TRIGGER_INTERVAL_SECS);
    let interval = Duration::from_secs(interval_secs);
    let state_for_task = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(TRIGGERWARE_START_DELAY_SECS)).await;
        loop {
            if let Err(err) = poll_all_companies(&state_for_task).await {
                tracing::warn!(error = ?err, "triggerware poll cycle failed");
            }
            tokio::time::sleep(interval).await;
        }
    });

    tracing::info!(interval_secs, "triggerware monitor armed");
}

pub async fn sync_company_monitors(
    state: &AppState,
    _user_id: ObjectId,
    company_id: ObjectId,
    group_name: &str,
    data_text: &str,
) -> Result<usize> {
    let Some(triggerware) = state.triggerware.as_ref() else {
        return Ok(0);
    };
    if !triggerware_has_connectors(triggerware).await? {
        return Ok(0);
    }
    let context = overview::make_chat_company_context(group_name, data_text);
    let schedule_secs = state
        .config
        .triggerware_poll_interval_secs
        .max(DEFAULT_TRIGGER_INTERVAL_SECS);
    let mut synced = 0usize;
    for spec in company_monitor_specs(company_id, &context, schedule_secs) {
        match triggerware
            .upsert_trigger(&spec.trigger_name, &spec.query, schedule_secs)
            .await
        {
            Ok(()) => {
                synced += 1;
            }
            Err(err) => {
                if is_triggerware_model_failure(&err) {
                    tracing::info!(
                        error = ?err,
                        trigger_name = %spec.trigger_name,
                        company_id = %company_id.to_hex(),
                        "triggerware could not synthesize this trigger; keeping Spec6-managed fallback active"
                    );
                } else {
                    tracing::warn!(
                        error = ?err,
                        trigger_name = %spec.trigger_name,
                        company_id = %company_id.to_hex(),
                        "triggerware sync failed"
                    );
                }
            }
        }
    }
    Ok(synced)
}

pub async fn sync_and_poll_company_monitors(
    state: &AppState,
    user_id: ObjectId,
    company_id: ObjectId,
    group_name: &str,
    data_text: &str,
) -> Result<TriggerSyncSummary> {
    let synced_triggers =
        sync_company_monitors(state, user_id, company_id, group_name, data_text).await?;
    let mut summary =
        poll_company_monitors(state, user_id, company_id, group_name, data_text).await?;
    summary.synced_triggers = synced_triggers;
    Ok(summary)
}

pub async fn list_company_trigger_events(
    db: &Db,
    user_id: ObjectId,
    company_id: ObjectId,
) -> Result<Vec<TriggerEventView>> {
    let mut cursor = db
        .trigger_events()
        .find(doc! { "user_id": user_id, "company_id": company_id }, None)
        .await
        .context("failed to query trigger events")?;
    let mut events = Vec::new();
    while let Some(doc) = cursor.try_next().await? {
        events.push(TriggerEventView {
            trigger_name: doc.trigger_name,
            trigger_kind: doc.trigger_kind,
            title: doc.title,
            body: doc.body,
            severity: doc.severity,
            delivered_channels: doc.delivered_channels,
            sources: doc.sources,
            created_at: doc.created_at,
        });
    }
    events.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(events)
}

pub async fn chat_query(config: &AppConfig, query: &str) -> Result<Vec<(String, Option<String>, String)>> {
    let api_key = config
        .triggerware_api_key
        .as_deref()
        .ok_or_else(|| anyhow!("TRIGGERWARE_API_KEY is not configured"))?;
    let client = TriggerwareClient::new(&config.triggerware_api_url, api_key)?;
    if !triggerware_has_connectors(&client).await? {
        return Err(anyhow!(
            "TriggerWare is connected but this instance has no installed connectors yet. Install connectors first, then Spec6 can run TriggerWare-backed alerts and queries."
        ));
    }
    let result = client.query_english(query).await?;
    Ok(extract_query_rows(&result))
}

async fn poll_all_companies(state: &AppState) -> Result<()> {
    let mut cursor = state.db.chat_groups().find(doc! {}, None).await?;
    while let Some(group) = cursor.try_next().await? {
        let Some(company_id) = group.id else {
            continue;
        };
        if !overview::should_queue_company_overview(&group.name, &group.data_text) {
            continue;
        }
        if let Err(err) = sync_and_poll_company_monitors(
            state,
            group.user_id,
            company_id,
            &group.name,
            &group.data_text,
        )
        .await
        {
            tracing::warn!(
                error = ?err,
                company_id = %company_id.to_hex(),
                "triggerware company sync/poll failed"
            );
        }
    }
    Ok(())
}

async fn poll_company_monitors(
    state: &AppState,
    user_id: ObjectId,
    company_id: ObjectId,
    group_name: &str,
    data_text: &str,
) -> Result<TriggerSyncSummary> {
    let Some(triggerware) = state.triggerware.as_ref() else {
        return Ok(TriggerSyncSummary::default());
    };
    if !triggerware_has_connectors(triggerware).await? {
        return Ok(TriggerSyncSummary::default());
    }

    let context = overview::make_chat_company_context(group_name, data_text);
    let specs = company_monitor_specs(
        company_id,
        &context,
        state.config
            .triggerware_poll_interval_secs
            .max(DEFAULT_TRIGGER_INTERVAL_SECS),
    );
    let mut summary = TriggerSyncSummary::default();
    let existing_trigger_names = triggerware
        .list_triggers()
        .await
        .map(|items| {
            items.into_iter()
                .map(|item| item.name)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();

    for spec in specs {
        if !existing_trigger_names.contains(&spec.trigger_name) {
            continue;
        }
        let delta = match triggerware.poll_trigger(&spec.trigger_name).await {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(
                    error = ?err,
                    trigger_name = %spec.trigger_name,
                    "triggerware poll failed"
                );
                continue;
            }
        };
        if delta.added.is_empty() && delta.deleted.is_empty() {
            continue;
        }

        let (title, body, severity, sources, payload) =
            summarize_trigger_delta(&context.company_name, &spec, &delta);
        let delivered_channels = notify_channels(state, &context.company_name, &title, &body, &severity, &sources).await;

        let event_doc = TriggerEventDoc {
            id: None,
            user_id,
            company_id,
            company_name: context.company_name.clone(),
            trigger_name: spec.trigger_name.clone(),
            trigger_kind: spec.kind.to_owned(),
            title: title.clone(),
            body: body.clone(),
            severity: severity.clone(),
            delivered_channels: delivered_channels.clone(),
            sources: sources.clone(),
            payload,
            created_at: Utc::now(),
        };
        state
            .db
            .trigger_events()
            .insert_one(event_doc, None)
            .await
            .context("failed to save trigger event")?;

        state.overview_events.emit(
            &company_id.to_hex(),
            "trigger_fired",
            &json!({
                "company_id": company_id.to_hex(),
                "trigger_name": spec.trigger_name,
                "trigger_kind": spec.kind,
                "severity": severity,
                "title": title,
            }),
        );

        ingest_trigger_to_cognee(
            state.cognee.clone(),
            &company_id.to_hex(),
            &context.company_name,
            &spec.trigger_name,
            &body,
            &severity,
        );

        maybe_refresh_overview(state, user_id, company_id).await?;
        summary.fired_events += 1;
    }

    Ok(summary)
}

fn company_monitor_specs(
    company_id: ObjectId,
    context: &overview::ChatCompanyContext,
    _schedule_secs: u64,
) -> Vec<CompanyMonitorSpec> {
    let profile = parse_profile(&context.data_text);
    let company = context.company_name.trim();
    let subject = if let Some(url) = profile.get("company url").or_else(|| profile.get("website")) {
        format!("{company} ({})", url.trim())
    } else {
        company.to_owned()
    };
    let specialty = profile
        .get("what do you specialize in?")
        .or_else(|| profile.get("specialty"))
        .cloned()
        .unwrap_or_default();
    let competitors = profile
        .get("known competitors")
        .cloned()
        .unwrap_or_default();

    let prefix = format!("spec6-{}-{}", company_id.to_hex(), slugify(company));
    vec![
        CompanyMonitorSpec {
            kind: "reputation",
            trigger_name: format!("{prefix}-reputation"),
            query: format!(
                "Create a monitor for {subject}. Detect new customer complaints, negative reviews, shipping issues, return friction, product quality complaints, scam accusations, and backlash. Use business context: {specialty}. Return source, title, summary, url, published_at, severity, and why this matters."
            ),
        },
        CompanyMonitorSpec {
            kind: "competition",
            trigger_name: format!("{prefix}-competition"),
            query: format!(
                "Create a monitor for {subject}. Detect competitor launches, pricing changes, comparison pages, promotions, product releases, hiring shifts, and major news. Known competitors: {competitors}. Return company, source, title, summary, url, published_at, and the competitive implication."
            ),
        },
        CompanyMonitorSpec {
            kind: "compliance",
            trigger_name: format!("{prefix}-compliance"),
            query: format!(
                "Create a monitor for {subject}. Detect recalls, lawsuits, fines, labor issues, privacy incidents, safety issues, sourcing controversies, and regulatory news. Return source, title, summary, url, published_at, compliance risk level, and likely business impact."
            ),
        },
    ]
}

fn parse_profile(data_text: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for line in data_text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_owned();
        if !key.is_empty() && !value.is_empty() {
            out.insert(key, value);
        }
    }
    out
}

fn is_triggerware_model_failure(err: &anyhow::Error) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    text.contains("model did not produce a trigger")
        || text.contains("model did not produce a query")
        || text.contains("failed to resolve")
}

async fn triggerware_has_connectors(client: &TriggerwareClient) -> Result<bool> {
    let installed = client.list_installed_connectors().await?;
    if installed.is_empty() {
        if !TRIGGERWARE_NO_CONNECTORS_WARNED.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                "triggerware is configured but this instance has no installed connectors. Install connectors in the TriggerWare console or via /connectors/installed/{{name}} before creating triggers."
            );
        }
        return Ok(false);
    }
    Ok(true)
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_owned()
}

fn summarize_trigger_delta(
    company_name: &str,
    spec: &CompanyMonitorSpec,
    delta: &TriggerDelta,
) -> (String, String, String, Vec<String>, Document) {
    let source_rows = delta
        .added
        .iter()
        .chain(delta.deleted.iter())
        .take(MAX_ROW_PREVIEW)
        .map(|row| render_row(row))
        .collect::<Vec<_>>();
    let sources = extract_sources(delta);
    let severity = detect_severity(spec.kind, &source_rows);
    let title = format!(
        "{} trigger fired for {} ({} new, {} removed)",
        spec.kind.to_ascii_uppercase(),
        company_name,
        delta.added.len(),
        delta.deleted.len()
    );
    let body = format!(
        "Triggerware detected change for {company_name}.\nKind: {}\nAdded rows: {}\nRemoved rows: {}\nTop rows:\n{}",
        spec.kind,
        delta.added.len(),
        delta.deleted.len(),
        source_rows
            .iter()
            .enumerate()
            .map(|(index, row)| format!("{}. {}", index + 1, row))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let payload = mongodb::bson::to_document(&json!({
        "added": delta.added,
        "deleted": delta.deleted,
    }))
    .unwrap_or_default();
    (title, body, severity, sources, payload)
}

fn render_row(row: &[Value]) -> String {
    row.iter()
        .map(value_to_string)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Array(values) => values.iter().map(value_to_string).collect::<Vec<_>>().join(", "),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| format!("{key}: {}", value_to_string(value)))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn detect_severity(kind: &str, rows: &[String]) -> String {
    let haystack = rows.join(" ").to_ascii_lowercase();
    if haystack.contains("lawsuit")
        || haystack.contains("recall")
        || haystack.contains("fine")
        || haystack.contains("fraud")
        || haystack.contains("breach")
        || haystack.contains("boycott")
    {
        return "high".to_owned();
    }
    match kind {
        "compliance" => "high".to_owned(),
        "reputation" => "medium".to_owned(),
        _ => "info".to_owned(),
    }
}

fn extract_sources(delta: &TriggerDelta) -> Vec<String> {
    let mut sources = Vec::new();
    for row in delta.added.iter().chain(delta.deleted.iter()) {
        for value in row {
            if let Some(url) = value.as_str().filter(|value| value.starts_with("http")) {
                sources.push(url.to_owned());
            }
        }
    }
    sources.sort();
    sources.dedup();
    sources.into_iter().take(8).collect()
}

async fn notify_channels(
    state: &AppState,
    company_name: &str,
    title: &str,
    body: &str,
    severity: &str,
    sources: &[String],
) -> Vec<String> {
    let http = Client::new();
    let mut delivered = Vec::new();
    let content = format!(
        "[{}] {}\n{}\nSources:\n{}",
        severity.to_ascii_uppercase(),
        title,
        body,
        if sources.is_empty() {
            "No direct URLs returned.".to_owned()
        } else {
            sources.join("\n")
        }
    );

    if let Some(url) = state.config.slack_webhook_url.as_deref() {
        let response = http
            .post(url)
            .json(&json!({
                "text": format!("Spec6 alert for {company_name}"),
                "blocks": [
                    {
                        "type": "header",
                        "text": { "type": "plain_text", "text": format!("Spec6 alert · {company_name}") }
                    },
                    {
                        "type": "section",
                        "text": { "type": "mrkdwn", "text": content }
                    }
                ]
            }))
            .send()
            .await;
        if response.as_ref().is_ok_and(|item| item.status().is_success()) {
            delivered.push("slack".to_owned());
        }
    }

    if let Some(url) = state.config.discord_webhook_url.as_deref() {
        let response = http
            .post(url)
            .json(&json!({
                "content": content,
            }))
            .send()
            .await;
        if response.as_ref().is_ok_and(|item| item.status().is_success()) {
            delivered.push("discord".to_owned());
        }
    }

    delivered
}

fn ingest_trigger_to_cognee(
    cognee: Option<Arc<CogneeClient>>,
    company_hex: &str,
    company_name: &str,
    trigger_name: &str,
    body: &str,
    severity: &str,
) {
    let Some(cognee) = cognee else {
        return;
    };
    let company_hex_owned = company_hex.to_owned();
    let dataset = cognee::dataset_name_for_group(&company_hex_owned);
    let filename = format!("trigger-{company_hex_owned}-{}", slugify(trigger_name));
    let memory = format!(
        "Spec6 trigger event\nCompany: {company_name}\nTrigger: {trigger_name}\nSeverity: {severity}\nTimestamp UTC: {}\n\n{}",
        Utc::now().to_rfc3339(),
        body
    );
    tokio::spawn(async move {
        if let Err(err) = cognee.ingest_and_cognify(&dataset, &memory, &filename).await {
            tracing::warn!("cognee trigger ingest failed for {company_hex_owned}: {err}");
        }
    });
}

async fn maybe_refresh_overview(state: &AppState, user_id: ObjectId, company_id: ObjectId) -> Result<()> {
    let should_refresh = match overview::load_company_overview(&state.db, user_id, company_id).await? {
        Some(overview) => overview.updated_at < Utc::now() - ChronoDuration::minutes(OVERVIEW_REFRESH_MINUTES_AFTER_DELTA),
        None => true,
    };
    if should_refresh {
        overview::queue_company_overview(state.clone(), user_id, company_id).await;
    }
    Ok(())
}

fn extract_query_rows(value: &Value) -> Vec<(String, Option<String>, String)> {
    match value {
        Value::Array(rows) => rows
            .iter()
            .map(|row| {
                let snippet = value_to_string(row);
                let url = row
                    .as_array()
                    .and_then(|values| {
                        values.iter().find_map(|item| {
                            item.as_str()
                                .filter(|value| value.starts_with("http"))
                                .map(str::to_owned)
                        })
                    });
                let title = row
                    .as_array()
                    .and_then(|values| values.first())
                    .map(value_to_string)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "Triggerware row".to_owned());
                (title, url, snippet)
            })
            .collect(),
        Value::Object(map) => map
            .get("rows")
            .map(extract_query_rows)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

async fn parse_response<T: for<'de> Deserialize<'de>>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("triggerware request failed {status}: {text}"));
    }
    serde_json::from_str(&text).with_context(|| format!("failed to parse triggerware response: {text}"))
}
