use crate::{
    chat,
    config::AppConfig,
    db::{CompanyOverviewDoc, Db},
    inference::{self, InferenceSelection},
    render::AppState,
};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use mongodb::bson::{Bson, doc, oid::ObjectId};
use reqwest::{StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{Mutex, broadcast},
    task,
    time::{sleep, timeout},
};

const MAX_MARKDOWN_CHARS: usize = 120_000;
const MAX_EVIDENCE_SNIPPET_CHARS: usize = 1200;
const MAX_COMPETITORS: usize = 4;
const MAX_REDDIT_URLS_PER_COMPETITOR: usize = 2;
const MAX_CUSTOMER_SERP_EVIDENCE_PER_QUERY: usize = 4;
const DISCOVERY_SERP_PAGES: usize = 1;
const CUSTOMER_SERP_PAGES: usize = 1;
const NEWS_SERP_PAGES: usize = 1;
const DISCOVERY_QUERY_LIMIT: usize = 8;
const NEWS_QUERY_LIMIT: usize = 3;
const CUSTOMER_QUERY_LIMIT: usize = 6;
const FOLLOWUP_QUERY_LIMIT: usize = 10;
const MAX_WEBSITE_PAGES_PER_COMPETITOR: usize = 8;
const MAX_COMPANY_SITE_PAGES: usize = 6;
const MAX_REVIEW_TARGETS_PER_COMPETITOR: usize = 4;
const MAX_CHAT_TOOL_OFFICIAL_DOMAINS: usize = 2;
const MAX_CHAT_TOOL_PAGES_PER_DOMAIN: usize = 4;
const COMPETITOR_ENRICH_TIMEOUT_SECS: u64 = 12;
const BRIGHTDATA_SERP_QUERY_CONCURRENCY: usize = 12;
const BRIGHTDATA_MAX_ATTEMPTS: usize = 1;
const BRIGHTDATA_RETRY_BASE_DELAY_MS: u64 = 700;
const BRIGHTDATA_HTTP_TIMEOUT_SECS: u64 = 8;
const OVERVIEW_DEBUG_DUMP_DIR: &str = "debug/overview-serp";
const GOOGLE_HOSTS: &[&str] = &["google.com", "google.co.uk", "google.ca", "google.com.au"];
const FILTERED_COMPETITOR_HOSTS: &[&str] = &[
    "wikipedia.org",
    "linkedin.com",
    "trustpilot.com",
    "g2.com",
    "capterra.com",
    "facebook.com",
    "instagram.com",
    "youtube.com",
    "reddit.com",
    "x.com",
    "twitter.com",
    "founderpal.ai",
    "similarweb.com",
    "semrush.com",
    "ahrefs.com",
    "crunchbase.com",
    "owler.com",
    "tracxn.com",
    "cbinsights.com",
    "producthunt.com",
    "alternativeto.net",
    "saasworthy.com",
    "getapp.com",
    "softwareadvice.com",
];
const AUTOMOTIVE_TERMS: &[&str] = &[
    "ford puma",
    "car market",
    "automotive",
    "auto",
    "vehicle",
    "vehicles",
    "suv",
    "crossover",
    "hatchback",
    "sedan",
    "mpg",
    "horsepower",
    "engine",
    "drivetrain",
    "road test",
    "car review",
];
const APPAREL_TERMS: &[&str] = &[
    "shoe",
    "shoes",
    "sneaker",
    "sneakers",
    "footwear",
    "apparel",
    "sportswear",
    "streetwear",
    "fashion",
    "football kit",
    "football kits",
    "jersey",
    "jerseys",
    "running",
    "cleats",
    "trainers",
];
const COMPLIANCE_TERMS: &[(&str, &str)] = &[
    ("privacy", "privacy and data handling"),
    ("gdpr", "privacy and data handling"),
    ("ccpa", "privacy and data handling"),
    ("data breach", "privacy and data handling"),
    ("terms", "terms and legal posture"),
    ("legal", "terms and legal posture"),
    ("warranty", "warranty posture"),
    ("return policy", "returns policy"),
    ("refund policy", "returns policy"),
    ("shipping policy", "shipping policy"),
    ("recall", "product recall or safety"),
    ("unsafe", "product recall or safety"),
    ("class action", "litigation risk"),
    ("lawsuit", "litigation risk"),
    ("fine", "regulatory pressure"),
    ("penalty", "regulatory pressure"),
    ("labor", "labor or sourcing risk"),
    ("supplier code", "supplier compliance"),
    ("code of conduct", "supplier compliance"),
    ("sustainability", "sustainability claims"),
    ("responsible sourcing", "supplier compliance"),
];

static OVERVIEW_EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompanyOverviewStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

impl CompanyOverviewStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    fn from_str(raw: &str) -> Self {
        match raw {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompanyOverviewSummary {
    pub actual_competitors: String,
    pub customer_trust_and_desire_to_use: String,
    pub faults: String,
    pub rating: String,
    pub where_to_do_better: String,
    pub how_long_this_will_last: String,
    pub market_saturation_and_overlap: String,
    pub confidence_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawSerpDump {
    pub query: String,
    pub source_type: String,
    pub items: Vec<RawSerpItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RawSerpItem {
    pub title: String,
    pub url: Option<String>,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OverviewEvidence {
    pub source_type: String,
    pub label: String,
    pub url: Option<String>,
    pub snippet: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OverviewCompetitor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
    pub classification: String,
    pub score: f64,
    pub overlap_summary: String,
    pub customer_trust: String,
    pub faults: String,
    pub rating_summary: String,
    pub where_to_do_better: String,
    pub durability_summary: String,
    pub saturation_summary: String,
    #[serde(default)]
    pub evidence: Vec<OverviewEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyOverview {
    pub company_id: String,
    pub company_name: String,
    pub status: CompanyOverviewStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub discovered_competitors: Vec<OverviewCompetitor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<CompanyOverviewSummary>,
    #[serde(default)]
    pub markdown_brief: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<CompanyOverviewDoc> for CompanyOverview {
    type Error = anyhow::Error;

    fn try_from(doc: CompanyOverviewDoc) -> Result<Self> {
        Ok(Self {
            company_id: doc
                .company_id
                .to_hex(),
            company_name: doc.company_name,
            status: CompanyOverviewStatus::from_str(&doc.status),
            started_at: doc.started_at,
            completed_at: doc.completed_at,
            discovered_competitors: doc.discovered_competitors,
            summary: doc.summary,
            markdown_brief: doc.markdown_brief,
            failure_reason: doc
                .failure_reason
                .and_then(|value| {
                    let trimmed = value.trim().to_owned();
                    if trimmed.is_empty() { None } else { Some(trimmed) }
                }),
            created_at: doc.created_at,
            updated_at: doc.updated_at,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewEventEnvelope {
    pub company_id: String,
    pub event: String,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct OverviewEventBus {
    sender: broadcast::Sender<OverviewEventEnvelope>,
}

impl OverviewEventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OverviewEventEnvelope> {
        self.sender.subscribe()
    }

    pub fn emit<T: Serialize>(&self, company_id: &str, event: &str, payload: &T) {
        let (_event_id, value) = overview_event_payload(payload);
        let _ = self.sender.send(OverviewEventEnvelope {
            company_id: company_id.to_owned(),
            event: event.to_owned(),
            payload: value,
        });
    }
}

#[derive(Debug, Clone, Default)]
pub struct OverviewRunGuard {
    active_company_ids: Arc<Mutex<HashSet<String>>>,
}

impl OverviewRunGuard {
    async fn try_acquire(&self, company_id: &str) -> bool {
        let mut guard = self.active_company_ids.lock().await;
        guard.insert(company_id.to_owned())
    }

    async fn release(&self, company_id: &str) {
        let mut guard = self.active_company_ids.lock().await;
        guard.remove(company_id);
    }
}

#[derive(Debug, Clone)]
struct BrightDataConfig {
    api_key: String,
    serp_zone: String,
    web_unlocker_zone: String,
    scraping_browser_auth: Option<String>,
    _proxy_url: Option<String>,
    trustpilot_dataset_id: Option<String>,
    g2_dataset_id: Option<String>,
    capterra_dataset_id: Option<String>,
    linkedin_company_dataset_id: String,
}

impl BrightDataConfig {
    fn from_app_config(config: &AppConfig) -> Result<Self> {
        let api_key = config
            .brightdata_api_key
            .clone()
            .ok_or_else(|| anyhow!("BRIGHTDATA_API_KEY is required for company overview runs"))?;
        let serp_zone = config
            .brightdata_serp_zone
            .clone()
            .ok_or_else(|| anyhow!("BRIGHTDATA_SERP_ZONE is required for company overview runs"))?;
        let web_unlocker_zone = config
            .brightdata_web_unlocker_zone
            .clone()
            .ok_or_else(|| {
                anyhow!("BRIGHTDATA_WEB_UNLOCKER_ZONE is required for company overview runs")
            })?;

        Ok(Self {
            api_key,
            serp_zone,
            web_unlocker_zone,
            scraping_browser_auth: config.brightdata_scraping_browser_auth.clone(),
            _proxy_url: config.brightdata_proxy_url.clone(),
            trustpilot_dataset_id: config.brightdata_trustpilot_dataset_id.clone(),
            g2_dataset_id: config.brightdata_g2_dataset_id.clone(),
            capterra_dataset_id: config.brightdata_capterra_dataset_id.clone(),
            linkedin_company_dataset_id: config
                .brightdata_linkedin_company_dataset_id
                .clone()
                .unwrap_or_else(|| "gd_l1vikfnt1wgvvqz95w".to_owned()),
        })
    }
}

#[derive(Debug, Clone, Default)]
struct CompanySeed {
    company_name: String,
    website: String,
    specialty: String,
    customers: String,
    known_competitors: String,
    notes: String,
}

#[derive(Debug, Clone)]
struct CandidateAccumulator {
    name: String,
    domain: Option<String>,
    website_url: Option<String>,
    score: f64,
    evidence: Vec<OverviewEvidence>,
    mentioned: bool,
}

pub async fn load_company_overview(
    db: &Db,
    user_id: ObjectId,
    company_id: ObjectId,
) -> Result<Option<CompanyOverview>> {
    let Some(doc) = db
        .company_overviews()
        .find_one(doc! { "user_id": user_id, "company_id": company_id }, None)
        .await
        .context("failed to load company overview")?
    else {
        return Ok(None);
    };

    CompanyOverview::try_from(doc).map(Some)
}

pub async fn delete_company_overview(
    db: &Db,
    user_id: ObjectId,
    company_id: ObjectId,
) -> Result<()> {
    db.company_overviews()
        .delete_one(doc! { "user_id": user_id, "company_id": company_id }, None)
        .await
        .context("failed to delete company overview")?;
    Ok(())
}

pub fn should_queue_company_overview(group_name: &str, data_text: &str) -> bool {
    let seed = parse_company_seed(group_name, data_text);
    !seed.company_name.trim().is_empty()
        || !seed.website.trim().is_empty()
        || !seed.specialty.trim().is_empty()
        || !seed.customers.trim().is_empty()
        || !seed.known_competitors.trim().is_empty()
        || !seed.notes.trim().is_empty()
}

pub fn prompt_context(group_data_text: Option<String>, overview: Option<&CompanyOverview>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(data_text) = group_data_text
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        parts.push(data_text);
    }

    if let Some(overview) = overview.filter(|item| item.status == CompanyOverviewStatus::Completed) {
        let mut section_lines = Vec::new();
        if let Some(summary) = &overview.summary {
            section_lines.push(format!("Actual competitors: {}", summary.actual_competitors));
            section_lines.push(format!(
                "Customer trust and desire to use: {}",
                summary.customer_trust_and_desire_to_use
            ));
            section_lines.push(format!("Faults: {}", summary.faults));
            section_lines.push(format!("Rating: {}", summary.rating));
            section_lines.push(format!("Where to do better: {}", summary.where_to_do_better));
            section_lines.push(format!(
                "How long this will last: {}",
                summary.how_long_this_will_last
            ));
            section_lines.push(format!(
                "Market saturation and overlap: {}",
                summary.market_saturation_and_overlap
            ));
            if !summary.confidence_notes.trim().is_empty() {
                section_lines.push(format!("Confidence notes: {}", summary.confidence_notes));
            }
        }

        let competitor_lines = overview
            .discovered_competitors
            .iter()
            .take(5)
            .map(|competitor| {
                let evidence = ranked_evidence(&competitor.evidence)
                    .into_iter()
                    .take(3)
                    .map(render_evidence_line)
                    .collect::<Vec<_>>()
                    .join("\n  ");
                format!(
                    "- {} ({})\n  Ratings: {}\n  Trust: {}\n  Faults: {}\n  Better: {}\n  Durability: {}\n  Evidence:\n  {}",
                    competitor.name,
                    competitor.classification,
                    competitor.rating_summary,
                    competitor.customer_trust,
                    competitor.faults,
                    competitor.where_to_do_better,
                    competitor.durability_summary,
                    evidence
                )
            })
            .collect::<Vec<_>>();

        let mut overview_parts = vec![
            "Sentinel competitor overview for this company. Use it as private context; do not mention hidden sourcing unless the user asks.".to_owned(),
        ];
        if !section_lines.is_empty() {
            overview_parts.push(section_lines.join("\n"));
        }
        if !competitor_lines.is_empty() {
            overview_parts.push(format!(
                "Ranked competitor notes:\n{}",
                competitor_lines.join("\n")
            ));
        }
        if !overview.markdown_brief.trim().is_empty() {
            overview_parts.push(format!(
                "Full saved analyst brief:\n{}",
                truncate(&overview.markdown_brief, 10_000)
            ));
        }
        parts.push(overview_parts.join("\n\n"));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

pub async fn chat_runtime_context(
    state: &AppState,
    user_id: ObjectId,
    conversation: &chat::Conversation,
    user_prompt: &str,
) -> Result<ChatRuntimeContextResult> {
    let Some(group_id) = conversation.group_id.as_deref() else {
        return Ok(ChatRuntimeContextResult::default());
    };
    let group_oid =
        ObjectId::parse_str(group_id).context("stored conversation group id invalid")?;
    let group = chat::load_chat_group(&state.db, user_id, group_oid).await?;
    let overview = load_company_overview(&state.db, user_id, group_oid).await?;
    let _ = user_prompt; // research now runs via the agentic tool loop, not pre-flight
    let base = prompt_context(group.map(|item| item.data_text), overview.as_ref());
    Ok(ChatRuntimeContextResult {
        prompt_context: base,
        tool_calls: Vec::new(),
    })
}

pub async fn queue_company_overview(state: AppState, user_id: ObjectId, company_id: ObjectId) {
    let company_hex = company_id.to_hex();
    if !state.overview_runs.try_acquire(&company_hex).await {
        return;
    }

    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(err) = run_company_overview(state_clone.clone(), user_id, company_id).await {
            tracing::error!(error = ?err, company_id = %company_hex, "company overview run failed");
            let failure_message = err.to_string();
            let _ = set_overview_failed(
                &state_clone.db,
                user_id,
                company_id,
                None,
                &failure_message,
            )
            .await;
            state_clone.overview_events.emit(
                &company_hex,
                "overview_error",
                &json!({
                    "company_id": company_hex,
                    "error": failure_message,
                    "status": "failed",
                }),
            );
        }
        state_clone.overview_runs.release(&company_hex).await;
    });
}

async fn run_company_overview(state: AppState, user_id: ObjectId, company_id: ObjectId) -> Result<()> {
    let company_hex = company_id.to_hex();
    let Some(group) = chat::load_chat_group(&state.db, user_id, company_id)
        .await
        .context("failed to load company for overview")?
    else {
        return Ok(());
    };
    let seed = parse_company_seed(&group.name, &group.data_text);
    let company_name = effective_company_name(&group.name, &seed);
    let brightdata = match BrightDataConfig::from_app_config(&state.config) {
        Ok(config) => config,
        Err(err) => {
            set_overview_failed(
                &state.db,
                user_id,
                company_id,
                Some(&company_name),
                &err.to_string(),
            )
            .await?;
            state.overview_events.emit(
                &company_hex,
                "overview_error",
                &json!({
                    "company_id": company_hex,
                    "status": "failed",
                    "error": err.to_string(),
                }),
            );
            return Ok(());
        }
    };
    let brand_website = normalize_website(&seed.website);
    let now = Utc::now();
    let debug_run_id = overview_debug_run_id(&company_hex, &company_name);

    set_overview_status(
        &state.db,
        user_id,
        company_id,
        &company_name,
        CompanyOverviewStatus::Queued,
        Some(now),
        None,
        None,
    )
    .await?;
    state.overview_events.emit(
        &company_hex,
        "overview_status",
        &json!({
            "company_id": company_hex,
            "status": "queued",
            "started_at": now,
            "company_name": company_name,
            "debug_run_id": debug_run_id,
        }),
    );

    set_overview_status(
        &state.db,
        user_id,
        company_id,
        &company_name,
        CompanyOverviewStatus::Running,
        Some(now),
        None,
        None,
    )
    .await?;
    state.overview_events.emit(
        &company_hex,
        "overview_status",
        &json!({
            "company_id": company_hex,
            "status": "running",
            "started_at": now,
            "company_name": company_name,
            "debug_run_id": debug_run_id,
        }),
    );

    let client = reqwest::Client::builder()
        .user_agent("win-win-sentinel/0.1")
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(BRIGHTDATA_HTTP_TIMEOUT_SECS))
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .context("failed to build overview http client")?;

    state.overview_events.emit(
        &company_hex,
        "source_started",
        &json!({ "company_id": company_hex, "source": "serp", "detail": "Discovering competitors and recent news" }),
    );

    let discovery = discover_competitors(
        &client,
        &brightdata,
        &company_name,
        brand_website.as_deref(),
        &seed,
    )
    .await?;
    match write_raw_serp_debug_dump(
        &company_hex,
        &company_name,
        &debug_run_id,
        &discovery.raw_serp_dumps,
    ) {
        Ok(path) => {
            tracing::info!(
                company_id = %company_hex,
                debug_run_id = %debug_run_id,
                path = %path.display(),
                "wrote raw SERP debug dump"
            );
        }
        Err(err) => {
            tracing::warn!(
                error = ?err,
                company_id = %company_hex,
                debug_run_id = %debug_run_id,
                "failed to write raw SERP debug dump"
            );
        }
    }

    state.overview_events.emit(
        &company_hex,
        "source_completed",
        &json!({
            "company_id": company_hex,
            "source": "serp",
            "detail": "Competitor discovery finished",
            "found": discovery.candidates.len(),
            "debug_run_id": debug_run_id,
        }),
    );

    // Emit a `competitor_found` event for each candidate up-front so the UI
    // populates the list immediately, then enrich them all in parallel.
    let candidates: Vec<_> = discovery.candidates.iter().take(MAX_COMPETITORS).cloned().collect();
    for candidate in &candidates {
        state.overview_events.emit(
            &company_hex,
            "competitor_found",
            &json!({
                "company_id": company_hex,
                "competitor": {
                    "name": candidate.name,
                    "domain": candidate.domain,
                    "website_url": candidate.website_url,
                    "classification": if candidate.mentioned { "mentioned" } else { "actual" },
                    "score": candidate.score,
                }
            }),
        );
    }

    let enrichment_futures = candidates.iter().map(|candidate| {
        async {
            match timeout(
                Duration::from_secs(COMPETITOR_ENRICH_TIMEOUT_SECS),
                enrich_competitor(
                    &state,
                    &client,
                    &brightdata,
                    &company_name,
                    &discovery,
                    candidate,
                ),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Ok(overview_competitor_from_evidence(
                    &company_name,
                    &discovery,
                    candidate,
                    candidate.evidence.clone(),
                )),
            }
        }
    });
    let competitors: Vec<OverviewCompetitor> = futures::future::try_join_all(enrichment_futures).await?;

    let heuristic_summary = build_heuristic_summary(&company_name, &discovery, &competitors);
    let cognee_memory = fetch_cognee_memory_context(&state, &company_hex, &company_name).await;
    let (summary, markdown_brief) = synthesize_overview(
        &state.config,
        &group.data_text,
        &company_name,
        &discovery,
        &competitors,
        &heuristic_summary,
        cognee_memory.as_deref(),
    )
    .await
    .unwrap_or_else(|err| {
        tracing::warn!(error = ?err, company_id = %company_hex, "overview synthesis failed, using heuristic fallback");
        (
            heuristic_summary.clone(),
            render_markdown_brief(&company_name, &heuristic_summary, &competitors, &discovery.raw_serp_dumps),
        )
    });

    let completed_at = Utc::now();
    let overview_doc = CompanyOverviewDoc {
        id: None,
        user_id,
        company_id,
        company_name: company_name.clone(),
        status: CompanyOverviewStatus::Completed.as_str().to_owned(),
        started_at: Some(now),
        completed_at: Some(completed_at),
        discovered_competitors: competitors.clone(),
        summary: Some(summary.clone()),
        markdown_brief: truncate_markdown(&markdown_brief),
        failure_reason: None,
        created_at: completed_at,
        updated_at: completed_at,
    };
    save_completed_overview(&state.db, overview_doc).await?;

    // Push the completed overview brief into Cognee so the knowledge graph
    // has the full competitive intelligence for this company.
    if let Some(cognee) = state.cognee.clone() {
        let dataset = crate::cognee::dataset_name_for_group(&company_hex);
        let brief_text = format!(
            "Company Intelligence Report: {company_name}\n\nStatus: Completed\n\nSummary:\n- Actual competitors: {}\n- Customer trust: {}\n- Faults: {}\n- Ratings: {}\n- Where to do better: {}\n- Durability: {}\n- Saturation: {}\n\nTop competitors:\n{}\n\nFull brief:\n{}",
            summary.actual_competitors,
            summary.customer_trust_and_desire_to_use,
            summary.faults,
            summary.rating,
            summary.where_to_do_better,
            summary.how_long_this_will_last,
            summary.market_saturation_and_overlap,
            competitors
                .iter()
                .map(|item| format!(
                    "- {} | class={} | score={:.1} | rating={} | trust={} | faults={}",
                    item.name,
                    item.classification,
                    item.score,
                    item.rating_summary,
                    item.customer_trust,
                    item.faults
                ))
                .collect::<Vec<_>>()
                .join("\n"),
            markdown_brief
        );
        let fname = format!("overview-{company_hex}");
        let company_hex_clone = company_hex.clone();
        tokio::spawn(async move {
            if let Err(e) = cognee.ingest_and_cognify(&dataset, &brief_text, &fname).await {
                tracing::warn!("cognee overview ingest failed for {company_hex_clone}: {e}");
            } else {
                tracing::info!("cognee: ingested overview for company {company_hex_clone}");
            }
        });
    }

    state.overview_events.emit(
        &company_hex,
        "overview_complete",
        &json!({
            "company_id": company_hex,
            "status": "completed",
            "completed_at": completed_at,
        }),
    );

    Ok(())
}

#[derive(Debug, Clone, Default)]
struct DiscoveryResult {
    candidates: Vec<CandidateAccumulator>,
    company_evidence: Vec<OverviewEvidence>,
    recent_news: Vec<OverviewEvidence>,
    review_page_urls: HashMap<String, HashMap<String, String>>,
    linkedin_company_urls: HashMap<String, String>,
    reddit_page_urls: HashMap<String, Vec<String>>,
    raw_serp_dumps: Vec<RawSerpDump>,
}

async fn discover_competitors(
    client: &reqwest::Client,
    brightdata: &BrightDataConfig,
    company_name: &str,
    brand_website: Option<&str>,
    seed: &CompanySeed,
) -> Result<DiscoveryResult> {
    let mut by_key: HashMap<String, CandidateAccumulator> = HashMap::new();
    let mut company_evidence = Vec::new();
    let mut recent_news = Vec::new();
    let mut review_page_urls: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut linkedin_company_urls = HashMap::new();
    let mut reddit_page_urls: HashMap<String, Vec<String>> = HashMap::new();
    let mut raw_serp_dumps = Vec::new();

    for known in split_competitor_names(&seed.known_competitors) {
        let key = normalized_key(&known);
        if key.is_empty() {
            continue;
        }
        by_key.entry(key).or_insert(CandidateAccumulator {
            name: known.clone(),
            domain: None,
            website_url: None,
            score: 2.0,
            evidence: vec![OverviewEvidence {
                source_type: "onboarding".to_owned(),
                label: "Known competitor from onboarding".to_owned(),
                url: None,
                snippet: format!("The founder listed {known} as a known competitor."),
                rating: None,
                review_count: None,
                metadata: Map::new(),
            }],
            mentioned: true,
        });
    }

    let search_queries = discovery_search_queries(company_name, seed)
        .into_iter()
        .take(DISCOVERY_QUERY_LIMIT)
        .collect::<Vec<_>>();

    let discovery_requests = search_queries
        .into_iter()
        .map(|query| GoogleSearchRequest {
            query,
            source_type: "serp".to_owned(),
            pages: DISCOVERY_SERP_PAGES,
            extra_params: None,
        })
        .collect::<Vec<_>>();
    for (query, source_type, results) in
        brightdata_google_search_many(client, brightdata, discovery_requests).await
    {
        push_raw_serp_dump(&mut raw_serp_dumps, &query, &source_type, &results);
        for item in results.items {
            if is_irrelevant_for_company_context(
                seed,
                brand_website,
                &item.title,
                &item.snippet,
                item.url.as_deref(),
            ) {
                continue;
            }
            let Some(host) = item
                .url
                .as_deref()
                .and_then(host_from_url)
            else {
                continue;
            };
            if should_ignore_competitor_host(&host, brand_website) || GOOGLE_HOSTS.contains(&host.as_str()) {
                continue;
            }
            if is_competitor_listicle_result(&item.title, &item.snippet) {
                continue;
            }

            let domain = Some(host.clone());
            let name = infer_result_name(&item.title, &host);
            if name
                .as_deref()
                .is_some_and(is_bad_competitor_candidate_name)
            {
                continue;
            }
            let key = candidate_key(name.as_deref(), domain.as_deref());
            if key.is_empty() {
                continue;
            }
            let candidate = by_key.entry(key).or_insert(CandidateAccumulator {
                name: name.unwrap_or_else(|| host.clone()),
                domain: domain.clone(),
                website_url: item.url.clone(),
                score: 0.0,
                evidence: Vec::new(),
                mentioned: false,
            });
            candidate.score += 3.0;
            if candidate.website_url.is_none() {
                candidate.website_url = item.url.clone();
            }
            if candidate.domain.is_none() {
                candidate.domain = domain;
            }
            candidate.evidence.push(OverviewEvidence {
                source_type: "serp".to_owned(),
                label: item.title.clone(),
                url: item.url.clone(),
                snippet: truncate(&item.snippet, MAX_EVIDENCE_SNIPPET_CHARS),
                rating: None,
                review_count: None,
                metadata: Map::new(),
            });
        }
    }

    let brand_host = brand_website.as_deref().and_then(host_from_url);
    let company_customer_requests = customer_search_queries_for_name(
        company_name,
        seed,
        company_name,
        brand_host.as_deref(),
    )
    .into_iter()
    .take(CUSTOMER_QUERY_LIMIT)
    .map(|(source, query)| GoogleSearchRequest {
        query,
        source_type: source.to_owned(),
        pages: CUSTOMER_SERP_PAGES,
        extra_params: None,
    })
    .collect::<Vec<_>>();
    for (query, source, results) in
        brightdata_google_search_many(client, brightdata, company_customer_requests).await
    {
        push_raw_serp_dump(&mut raw_serp_dumps, &query, &source, &results);
        for item in results.items.into_iter().take(MAX_CUSTOMER_SERP_EVIDENCE_PER_QUERY) {
            if is_irrelevant_for_company_context(
                seed,
                brand_website,
                &item.title,
                &item.snippet,
                item.url.as_deref(),
            ) {
                continue;
            }
            let exact_source_match = customer_result_matches_source(&source, item.url.as_deref());
            let evidence_source = if exact_source_match {
                source.as_str()
            } else if is_customer_intent_result(&item) {
                "serp_review"
            } else {
                continue;
            };
            company_evidence.push(customer_serp_evidence(evidence_source, item));
        }
    }

    if let Some(base_url) = brand_website {
        for (label, page_url) in official_site_page_targets(base_url, MAX_COMPANY_SITE_PAGES) {
            let Ok(markdown) = web_unlocker_markdown(client, brightdata, &page_url, None).await
            else {
                continue;
            };
            if markdown.trim().is_empty() {
                continue;
            }
            company_evidence.push(website_evidence_from_markdown(&label, &page_url, &markdown));
        }
    }

    let news_requests = news_search_queries(company_name, seed)
        .into_iter()
        .take(NEWS_QUERY_LIMIT)
        .map(|query| GoogleSearchRequest {
            query,
            source_type: "news".to_owned(),
            pages: NEWS_SERP_PAGES,
            extra_params: Some("tbm=nws&tbs=qdr:m6".to_owned()),
        })
        .collect::<Vec<_>>();
    for (query, source_type, results) in
        brightdata_google_search_many(client, brightdata, news_requests).await
    {
        push_raw_serp_dump(&mut raw_serp_dumps, &query, &source_type, &results);
        for item in results.items {
            if is_irrelevant_for_company_context(
                seed,
                brand_website,
                &item.title,
                &item.snippet,
                item.url.as_deref(),
            ) {
                continue;
            }
            recent_news.push(OverviewEvidence {
                source_type: "news".to_owned(),
                label: item.title,
                url: item.url,
                snippet: truncate(&item.snippet, MAX_EVIDENCE_SNIPPET_CHARS),
                rating: None,
                review_count: None,
                metadata: Map::new(),
            });
        }
    }

    let mut candidate_keys = by_key
        .iter()
        .map(|(key, candidate)| (key.clone(), candidate.score))
        .collect::<Vec<_>>();
    candidate_keys.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    let candidate_keys = candidate_keys
        .into_iter()
        .take(MAX_COMPETITORS)
        .map(|(key, _)| key)
        .collect::<Vec<_>>();

    for key in &candidate_keys {
        let Some(competitor) = by_key.get_mut(key) else {
            continue;
        };
        let customer_queries = customer_search_queries(company_name, seed, competitor);

        let customer_requests = customer_queries
            .into_iter()
            .take(CUSTOMER_QUERY_LIMIT)
            .map(|(source, query)| GoogleSearchRequest {
                query,
                source_type: source.to_owned(),
                pages: if source == "linkedin" { 1 } else { CUSTOMER_SERP_PAGES },
                extra_params: None,
            })
            .collect::<Vec<_>>();
        for (query, source, results) in
            brightdata_google_search_many(client, brightdata, customer_requests).await
        {
            push_raw_serp_dump(&mut raw_serp_dumps, &query, &source, &results);
            let source = source.as_str();

            for item in results.items.into_iter().take(MAX_CUSTOMER_SERP_EVIDENCE_PER_QUERY) {
                if is_irrelevant_for_company_context(
                    seed,
                    brand_website,
                    &item.title,
                    &item.snippet,
                    item.url.as_deref(),
                ) {
                    continue;
                }
                let exact_source_match = customer_result_matches_source(source, item.url.as_deref());
                let evidence_source = if exact_source_match {
                    source
                } else if source == "linkedin" {
                    continue;
                } else if is_customer_intent_result(&item) {
                    "serp_review"
                } else {
                    continue;
                };
                if let Some(found_url) = item.url.clone() {
                    match evidence_source {
                        "linkedin" => {
                            linkedin_company_urls
                                .entry(competitor.name.clone())
                                .or_insert(found_url);
                        }
                        "reddit" if found_url.contains("reddit.com/r/") => {
                            let urls = reddit_page_urls
                                .entry(competitor.name.clone())
                                .or_default();
                            if urls.len() < MAX_REDDIT_URLS_PER_COMPETITOR
                                && !urls.contains(&found_url)
                            {
                                urls.push(found_url.clone());
                            }
                        }
                        "trustpilot" | "g2" | "capterra" => {
                            review_page_urls
                                .entry(competitor.name.clone())
                                .or_default()
                                .entry(source.to_owned())
                                .or_insert(found_url.clone());
                        }
                        _ => {}
                    }
                }

                if evidence_source != "linkedin" {
                    competitor.score += customer_evidence_score(evidence_source);
                    competitor.evidence.push(customer_serp_evidence(evidence_source, item));
                }
            }
        }
    }

    run_followup_research(
        client,
        brightdata,
        company_name,
        seed,
        brand_website,
        &candidate_keys,
        &mut by_key,
        &mut company_evidence,
        &mut raw_serp_dumps,
    )
    .await;

    let mut candidates = by_key.into_values().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.name.cmp(&right.name))
    });

    Ok(DiscoveryResult {
        candidates,
        company_evidence,
        recent_news,
        review_page_urls,
        linkedin_company_urls,
        reddit_page_urls,
        raw_serp_dumps,
    })
}

async fn enrich_competitor(
    _state: &AppState,
    client: &reqwest::Client,
    brightdata: &BrightDataConfig,
    company_name: &str,
    discovery: &DiscoveryResult,
    candidate: &CandidateAccumulator,
) -> Result<OverviewCompetitor> {
    let mut evidence = candidate.evidence.clone();
    let website_url = candidate
        .website_url
        .clone()
        .or_else(|| {
            candidate
                .domain
                .as_deref()
                .map(|domain| format!("https://{}", domain.trim_start_matches("www.")))
        });

    if let Some(base_url) = website_url.as_deref() {
        for (label, page_url) in
            official_site_page_targets(base_url, MAX_WEBSITE_PAGES_PER_COMPETITOR)
        {
            let Ok(markdown) = web_unlocker_markdown(client, brightdata, &page_url, None)
                .await
            else {
                continue;
            };
            if markdown.trim().is_empty() {
                continue;
            }
            evidence.push(website_evidence_from_markdown(&label, &page_url, &markdown));
        }
    }

    for (source, url) in review_targets(discovery, candidate)
        .into_iter()
        .take(MAX_REVIEW_TARGETS_PER_COMPETITOR)
    {
        let dataset_id = match source.as_str() {
            "trustpilot" => brightdata.trustpilot_dataset_id.as_deref(),
            "g2" => brightdata.g2_dataset_id.as_deref(),
            "capterra" => brightdata.capterra_dataset_id.as_deref(),
            _ => None,
        };
        if let Some(dataset_id) = dataset_id {
            if let Ok(value) = dataset_scrape(client, &brightdata.api_key, dataset_id, &url).await {
                evidence.push(review_evidence_from_value(&source, &url, &value));
                continue;
            }
        }
        if let Ok(markdown) = web_unlocker_markdown(client, brightdata, &url, None)
            .await
        {
            evidence.push(review_evidence_from_markdown(&source, &url, &markdown));
        }
    }

    if let Some(linkedin_url) = discovery.linkedin_company_urls.get(&candidate.name) {
        if let Ok(value) = dataset_scrape(
            client,
            &brightdata.api_key,
            &brightdata.linkedin_company_dataset_id,
            linkedin_url,
        )
        .await
        {
            evidence.push(linkedin_evidence_from_value(linkedin_url, &value));
        }
    }

    if let Some(reddit_urls) = discovery.reddit_page_urls.get(&candidate.name) {
        for reddit_url in reddit_urls.iter().take(MAX_REDDIT_URLS_PER_COMPETITOR) {
            if let Ok(markdown) = web_unlocker_markdown(client, brightdata, reddit_url, None).await
            {
                evidence.push(reddit_evidence_from_markdown(reddit_url, &markdown));
            }
        }
    }

    Ok(overview_competitor_from_evidence(
        company_name,
        discovery,
        candidate,
        evidence,
    ))
}

fn overview_competitor_from_evidence(
    company_name: &str,
    discovery: &DiscoveryResult,
    candidate: &CandidateAccumulator,
    evidence: Vec<OverviewEvidence>,
) -> OverviewCompetitor {
    let trust_score = average_rating(&evidence);
    let review_count = total_review_count(&evidence);
    let (positive_signals, negative_signals) = classify_review_text(&evidence);
    let company_mentions_in_news = discovery
        .recent_news
        .iter()
        .filter(|item| snippet_mentions(&item.snippet, &candidate.name))
        .count();

    let has_review_evidence = evidence.iter().any(|item| {
        matches!(
            item.source_type.as_str(),
            "trustpilot" | "g2" | "capterra" | "reddit" | "customer_search"
        )
    });
    let has_overlap_signal = evidence.iter().any(|item| {
        let text = item.snippet.to_ascii_lowercase();
        text.contains("alternative")
            || text.contains("alternatives")
            || text.contains("competitor")
            || text.contains("compare")
            || text.contains("comparison")
            || text.contains("vs ")
    });
    let classification = if has_review_evidence
        || (candidate.domain.is_some() && has_overlap_signal)
        || (candidate.score >= 4.5 && !candidate.evidence.iter().all(|item| item.source_type == "onboarding"))
    {
        "actual"
    } else {
        "mentioned"
    };

    OverviewCompetitor {
        name: candidate.name.clone(),
        domain: candidate.domain.clone(),
        website_url: candidate.website_url.clone(),
        classification: classification.to_owned(),
        score: candidate.score,
        overlap_summary: overlap_summary(company_name, candidate, &evidence),
        customer_trust: customer_trust_summary(
            trust_score,
            review_count,
            &evidence,
            &positive_signals,
            &negative_signals,
        ),
        faults: faults_summary(&negative_signals, &evidence),
        rating_summary: rating_summary(trust_score, review_count, &evidence),
        where_to_do_better: improvement_summary(&negative_signals, &evidence),
        durability_summary: durability_summary(company_mentions_in_news, &evidence),
        saturation_summary: saturation_summary(classification, candidate.score, candidate.mentioned),
        evidence,
    }
}

async fn run_followup_research(
    client: &reqwest::Client,
    brightdata: &BrightDataConfig,
    company_name: &str,
    seed: &CompanySeed,
    brand_website: Option<&str>,
    candidate_keys: &[String],
    by_key: &mut HashMap<String, CandidateAccumulator>,
    company_evidence: &mut Vec<OverviewEvidence>,
    raw_serp_dumps: &mut Vec<RawSerpDump>,
) {
    let requests = build_followup_requests(
        company_name,
        seed,
        candidate_keys,
        by_key,
        company_evidence,
    );
    if requests.is_empty() {
        return;
    }

    let results = futures::stream::iter(requests)
        .map(|request| async move {
            let results = brightdata_google_search_items(
                client,
                brightdata,
                &request.query,
                &request.source_type,
                CUSTOMER_SERP_PAGES,
                None,
            )
            .await
            .unwrap_or_else(|_| GoogleSearchResults { items: Vec::new() });
            (request, results)
        })
        .buffer_unordered(BRIGHTDATA_SERP_QUERY_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;

    for (request, results) in results {
        push_raw_serp_dump(raw_serp_dumps, &request.query, &request.source_type, &results);
        match request.target {
            FollowupTarget::Company => {
                for item in results.items.into_iter().take(MAX_CUSTOMER_SERP_EVIDENCE_PER_QUERY) {
                    if is_irrelevant_for_company_context(
                        seed,
                        brand_website,
                        &item.title,
                        &item.snippet,
                        item.url.as_deref(),
                    ) {
                        continue;
                    }
                    let evidence_source = followup_evidence_source(&request.source_type, &item);
                    company_evidence.push(customer_serp_evidence(evidence_source, item));
                }
            }
            FollowupTarget::Competitor(key) => {
                let Some(candidate) = by_key.get_mut(&key) else {
                    continue;
                };
                for item in results.items.into_iter().take(MAX_CUSTOMER_SERP_EVIDENCE_PER_QUERY) {
                    if is_irrelevant_for_company_context(
                        seed,
                        brand_website,
                        &item.title,
                        &item.snippet,
                        item.url.as_deref(),
                    ) {
                        continue;
                    }
                    let evidence_source = followup_evidence_source(&request.source_type, &item);
                    candidate.score += customer_evidence_score(evidence_source);
                    candidate.evidence.push(customer_serp_evidence(evidence_source, item));
                }
            }
        }
    }
}

fn build_followup_requests(
    company_name: &str,
    seed: &CompanySeed,
    candidate_keys: &[String],
    by_key: &HashMap<String, CandidateAccumulator>,
    company_evidence: &[OverviewEvidence],
) -> Vec<FollowupSearchRequest> {
    let mut requests = Vec::new();
    let software_context = is_software_review_context(seed);
    let company_subject = primary_search_subject(company_name, seed);

    if average_rating(company_evidence).is_none() {
        requests.push(FollowupSearchRequest {
            target: FollowupTarget::Company,
            source_type: "trustpilot".to_owned(),
            query: format!("{company_subject} Trustpilot rating reviews"),
        });
    }
    if classify_review_text(company_evidence).1.is_empty() {
        requests.push(FollowupSearchRequest {
            target: FollowupTarget::Company,
            source_type: "serp_review".to_owned(),
            query: format!("{company_subject} complaints returns quality customer service"),
        });
    }

    for key in candidate_keys {
        let Some(candidate) = by_key.get(key) else {
            continue;
        };
        let name = candidate.name.trim();
        if average_rating(&candidate.evidence).is_none() {
            requests.push(FollowupSearchRequest {
                target: FollowupTarget::Competitor(key.clone()),
                source_type: "trustpilot".to_owned(),
                query: format!("{name} Trustpilot rating reviews"),
            });
        }
        if classify_review_text(&candidate.evidence).1.is_empty() {
            requests.push(FollowupSearchRequest {
                target: FollowupTarget::Competitor(key.clone()),
                source_type: "serp_review".to_owned(),
                query: format!("{name} complaints quality returns customer service"),
            });
        }
        requests.push(FollowupSearchRequest {
            target: FollowupTarget::Competitor(key.clone()),
            source_type: "serp_review".to_owned(),
            query: format!("{company_name} vs {name} customer reviews quality durability"),
        });
        if software_context {
            requests.push(FollowupSearchRequest {
                target: FollowupTarget::Competitor(key.clone()),
                source_type: "g2".to_owned(),
                query: format!("site:g2.com/products {name} reviews pros cons"),
            });
        }
    }

    dedupe_followup_requests(requests)
        .into_iter()
        .take(FOLLOWUP_QUERY_LIMIT)
        .collect()
}

fn followup_evidence_source<'a>(requested_source: &'a str, item: &SerpResultItem) -> &'a str {
    if customer_result_matches_source(requested_source, item.url.as_deref()) {
        requested_source
    } else if requested_source == "reddit"
        && item
            .url
            .as_deref()
            .is_some_and(|url| url.contains("reddit.com/r/"))
    {
        "reddit"
    } else {
        "serp_review"
    }
}

fn dedupe_followup_requests(requests: Vec<FollowupSearchRequest>) -> Vec<FollowupSearchRequest> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for request in requests {
        let target = match &request.target {
            FollowupTarget::Company => "company".to_owned(),
            FollowupTarget::Competitor(key) => key.clone(),
        };
        let key = format!("{target}:{}:{}", request.source_type, normalized_key(&request.query));
        if seen.insert(key) {
            out.push(request);
        }
    }
    out
}

async fn generate_chat_research_queries(
    config: &AppConfig,
    company_name: &str,
    seed: &CompanySeed,
    overview: Option<&CompanyOverview>,
    user_prompt: &str,
) -> Vec<ChatResearchQuery> {
    let heuristic = fallback_chat_research_queries(company_name, seed, overview, user_prompt);
    let Some(provider) = config.default_inference_provider() else {
        return heuristic;
    };
    let selection = InferenceSelection {
        provider,
        model: config.configured_or_default_model(provider),
    };
    let competitor_names = overview
        .map(|item| {
            item.discovered_competitors
                .iter()
                .take(4)
                .map(|competitor| competitor.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let summary = overview
        .and_then(|item| item.summary.as_ref())
        .map(|item| {
            format!(
                "Actual competitors: {}\nRatings: {}\nFaults: {}\nWhere to do better: {}",
                item.actual_competitors, item.rating, item.faults, item.where_to_do_better
            )
        })
        .unwrap_or_default();
    let system_prompt = "You are planning web research queries for a competitive intelligence assistant. Return ONLY a JSON array. Each item must be {\"query\": string, \"source_type\": \"trustpilot\"|\"reddit\"|\"serp_review\"|\"g2\"|\"capterra\"}. Propose at most 4 queries that would best answer the user message using public evidence.";
    let user_prompt = format!(
        "Company: {company_name}\nUser request: {user_prompt}\nKnown competitors: {competitor_names}\nOnboarding specialty: {}\nExisting summary:\n{}\nReturn only JSON.",
        seed.specialty,
        summary
    );
    let generated = timeout(
        Duration::from_secs(4),
        inference::generate_text(config, &selection, system_prompt, &user_prompt),
    )
    .await
    .ok()
    .and_then(Result::ok);
    let Some(generated) = generated else {
        return heuristic;
    };

    parse_chat_research_queries(&generated).unwrap_or(heuristic)
}

fn parse_chat_research_queries(raw: &str) -> Option<Vec<ChatResearchQuery>> {
    let trimmed = raw.trim();
    let array_slice = if trimmed.starts_with('[') {
        trimmed
    } else {
        let start = trimmed.find('[')?;
        let end = trimmed.rfind(']')?;
        &trimmed[start..=end]
    };
    let parsed = serde_json::from_str::<Vec<ChatResearchQuery>>(array_slice).ok()?;
    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

fn fallback_chat_research_queries(
    company_name: &str,
    seed: &CompanySeed,
    overview: Option<&CompanyOverview>,
    user_prompt: &str,
) -> Vec<ChatResearchQuery> {
    let mut out = Vec::new();
    let lower = user_prompt.to_ascii_lowercase();
    let competitors = overview
        .map(|item| {
            item.discovered_competitors
                .iter()
                .take(2)
                .map(|competitor| competitor.name.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| split_competitor_names(&seed.known_competitors).into_iter().take(2).collect());

    if lower.contains("rating") || lower.contains("review") || lower.contains("trust") {
        out.push(ChatResearchQuery {
            query: format!("{company_name} Trustpilot rating reviews"),
            source_type: Some("trustpilot".to_owned()),
        });
    }
    for competitor in competitors {
        if lower.contains("table") || lower.contains("compare") || lower.contains("better") {
            out.push(ChatResearchQuery {
                query: format!("{company_name} vs {competitor} customer reviews quality pricing"),
                source_type: Some("serp_review".to_owned()),
            });
        } else {
            out.push(ChatResearchQuery {
                query: format!("{competitor} Trustpilot rating reviews"),
                source_type: Some("trustpilot".to_owned()),
            });
        }
    }
    if out.is_empty() {
        out.push(ChatResearchQuery {
            query: format!("{company_name} competitor reviews comparison"),
            source_type: Some("serp_review".to_owned()),
        });
    }
    out
}

fn normalize_chat_source_type(source_type: Option<&str>) -> String {
    match source_type.unwrap_or("serp_review").trim().to_ascii_lowercase().as_str() {
        "trustpilot" => "trustpilot".to_owned(),
        "reddit" => "reddit".to_owned(),
        "g2" => "g2".to_owned(),
        "capterra" => "capterra".to_owned(),
        "amazon" => "amazon".to_owned(),
        "aliexpress" => "aliexpress".to_owned(),
        "ebay" => "ebay".to_owned(),
        "niche" => "niche".to_owned(),
        "news" => "news".to_owned(),
        "linkedin" => "linkedin".to_owned(),
        "social" => "social".to_owned(),
        "media" | "video" | "youtube" | "podcast" | "earnings_call" => "media".to_owned(),
        _ => "serp_review".to_owned(),
    }
}

/// Wrap a user query with a marketplace/site filter so a generic SERP call
/// effectively becomes a marketplace-specific scout. This is the cheap path —
/// production swaps these to BrightData's prebuilt marketplace scrapers.
fn marketplace_query_for(source_type: &str, query: &str) -> String {
    let q = query.trim();
    match source_type {
        "amazon" => format!("site:amazon.com \"{q}\" (fake OR counterfeit OR replica OR knockoff)"),
        "aliexpress" => format!("(site:aliexpress.com OR site:temu.com) \"{q}\" (replica OR knockoff OR fake)"),
        "ebay" => format!("(site:ebay.com OR site:etsy.com) \"{q}\" (replica OR knockoff OR counterfeit)"),
        "niche" => format!("(site:mercari.com OR site:depop.com OR site:vinted.com OR site:rakuten.co.jp) \"{q}\" (replica OR fake)"),
        "linkedin" => format!("site:linkedin.com \"{q}\" (hire OR joined OR launched OR pricing)"),
        "news" => format!("\"{q}\" news (recent OR launch OR risk OR sanctions OR funding)"),
        "social" => format!("(site:tiktok.com OR site:x.com OR site:youtube.com) \"{q}\"") ,
        "media" => format!("site:youtube.com {q} (review OR interview OR earnings OR analysis)"),
        _ => q.to_owned(),
    }
}

/// The "Spoken Web" scout. Most brand-critical signal now breaks in *audio* —
/// earnings calls, YouTube reviews, podcasts, TikTok — which text scrapers are
/// blind to. This tool: (1) discovers a relevant media URL via Bright Data SERP,
/// (2) batch-transcribes it with Speechmatics, (3) returns timestamped spoken
/// quotes as first-class evidence for the dossier.
async fn run_spoken_web_tool(
    config: &AppConfig,
    query: &str,
    started: std::time::Instant,
) -> ChatToolOutcome {
    let fail = |error: String| ChatToolOutcome {
        source_type: "media".to_owned(),
        query: query.to_owned(),
        items: Vec::new(),
        elapsed_ms: started.elapsed().as_millis(),
        error: Some(error),
    };

    let Some(api_key) = config.speechmatics_api_key.clone() else {
        return fail("Speechmatics not configured (SPEECHMATICS_API_KEY)".to_owned());
    };
    let brightdata = match BrightDataConfig::from_app_config(config) {
        Ok(value) => value,
        Err(err) => return fail(err.to_string()),
    };
    let client = match reqwest::Client::builder()
        .user_agent("win-win-sentinel/0.1")
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(BRIGHTDATA_HTTP_TIMEOUT_SECS))
        .build()
    {
        Ok(value) => value,
        Err(err) => return fail(format!("http client init failed: {err}")),
    };

    // 1. Prefer a direct media URL if the user supplied one. Otherwise
    //    discover candidate media via SERP.
    let mut items: Vec<ToolResultItem> = Vec::new();
    let (media_url, source_title) = if let Some(url) =
        extract_media_url_from_query(query).filter(|url| is_transcribable_media_url(url))
    {
        (url, "Provided media URL".to_owned())
    } else {
        let discovery_query = marketplace_query_for("media", query);
        let results = match brightdata_google_search_items(
            &client,
            &brightdata,
            &discovery_query,
            "media",
            1,
            None,
        )
        .await
        {
            Ok(results) => results,
            Err(err) => return fail(format!("media discovery failed: {err}")),
        };

        items.extend(results.items.iter().take(3).map(|item| ToolResultItem {
            title: item.title.clone(),
            url: item.url.clone(),
            snippet: truncate(&item.snippet, 280),
        }));

        let media_url = results
            .items
            .iter()
            .filter_map(|item| item.url.clone())
            .find(|url| is_transcribable_media_url(url));
        let Some(media_url) = media_url else {
            return fail("no transcribable media found for this query".to_owned());
        };
        let source_title = results
            .items
            .iter()
            .find(|item| item.url.as_deref() == Some(media_url.as_str()))
            .map(|item| item.title.clone())
            .unwrap_or_else(|| "Spoken-web source".to_owned());
        (media_url, source_title)
    };

    // 2. Transcribe with Speechmatics (bounded so the agent loop stays snappy).
    let (full, segments) = match crate::speechmatics::transcribe_url(
        &api_key,
        &config.speechmatics_batch_url,
        &media_url,
        90,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => return fail(format!("transcription failed: {err}")),
    };

    if full.trim().is_empty() {
        return fail("transcript was empty".to_owned());
    }

    // 3. Surface a transcript digest plus the most quotable timestamped segments.
    items.push(ToolResultItem {
        title: format!("Transcript · {source_title}"),
        url: Some(media_url.clone()),
        snippet: truncate(&full, MAX_EVIDENCE_SNIPPET_CHARS),
    });
    for seg in pick_quotable_segments(&segments, 5) {
        items.push(ToolResultItem {
            title: format!("[{}] {source_title}", seg.timestamp()),
            url: Some(media_url.clone()),
            snippet: truncate(&seg.text, 400),
        });
    }

    ChatToolOutcome {
        source_type: "media".to_owned(),
        query: query.to_owned(),
        items,
        elapsed_ms: started.elapsed().as_millis(),
        error: None,
    }
}

fn is_transcribable_media_url(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    u.contains("youtube.com/watch")
        || u.contains("youtu.be/")
        || u.ends_with(".mp3")
        || u.ends_with(".mp4")
        || u.ends_with(".m4a")
        || u.ends_with(".wav")
}

fn extract_media_url_from_query(query: &str) -> Option<String> {
    query.split_whitespace().find_map(|part| {
        let candidate = part
            .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '(' | ')' | ',' | ';'))
            .trim_end_matches('.');
        if candidate.starts_with("http://") || candidate.starts_with("https://") {
            Some(candidate.to_owned())
        } else {
            None
        }
    })
}

/// Prefer segments that carry substance (numbers, sentiment, named risk) so the
/// quotes we cite are the ones an analyst would pull.
fn pick_quotable_segments(
    segments: &[crate::speechmatics::TranscriptSegment],
    limit: usize,
) -> Vec<crate::speechmatics::TranscriptSegment> {
    const SIGNAL: &[&str] = &[
        "revenue", "growth", "decline", "percent", "%", "quarter", "guidance",
        "demand", "weak", "strong", "risk", "recall", "fake", "counterfeit",
        "lawsuit", "boycott", "launch", "margin", "billion", "million", "fy",
        "disappoint", "concern", "warn", "best", "worst", "love", "hate",
    ];
    let mut scored: Vec<(usize, &crate::speechmatics::TranscriptSegment)> = segments
        .iter()
        .map(|s| {
            let lower = s.text.to_ascii_lowercase();
            let score = SIGNAL.iter().filter(|k| lower.contains(*k)).count();
            (score, s)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
        .into_iter()
        .filter(|(score, _)| *score > 0)
        .take(limit)
        .map(|(_, s)| s.clone())
        .collect()
}

fn format_chat_tool_call_label(query: &str, source_type: &str) -> String {
    let q = query.trim();
    if source_type == "trustpilot" && q.contains("Trustpilot") {
        return q.replace("Trustpilot", "").trim().to_owned() + " Trustpilot";
    }
    if q.contains(" vs ") {
        return q.to_owned();
    }
    match source_type {
        "trustpilot" => format!("Checking ratings for {q}"),
        "reddit" => format!("Scanning Reddit for {q}"),
        "g2" => format!("Checking G2 for {q}"),
        "capterra" => format!("Checking Capterra for {q}"),
        _ => format!("Searching public reviews for {q}"),
    }
}

fn render_chat_research_block(
    user_prompt: &str,
    results: &[(String, String, GoogleSearchResults)],
) -> String {
    let mut lines = vec![
        "Live research for this user request. Use it as fresh private context and cite uncertainty when evidence is thin.".to_owned(),
        format!("User request: {user_prompt}"),
    ];
    let mut any = false;
    for (query, source_type, results) in results {
        if results.items.is_empty() {
            continue;
        }
        any = true;
        lines.push(format!("Query [{}]: {}", source_type, query));
        for item in results.items.iter().take(3) {
            lines.push(format!(
                "- {} | url={} | snippet={}",
                item.title,
                item.url.as_deref().unwrap_or("none"),
                truncate(&item.snippet.replace('\n', " "), 400)
            ));
        }
    }
    if any {
        lines.join("\n")
    } else {
        String::new()
    }
}

fn should_run_chat_research(user_prompt: &str) -> bool {
    let text = user_prompt.to_ascii_lowercase();
    [
        "compare",
        "comparison",
        "competitor",
        "competitors",
        "rating",
        "ratings",
        "review",
        "reviews",
        "trust",
        "fault",
        "complaint",
        "better",
        "worse",
        "pricing",
        "table",
        "why",
        "how",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

async fn synthesize_overview(
    config: &AppConfig,
    group_data_text: &str,
    company_name: &str,
    discovery: &DiscoveryResult,
    competitors: &[OverviewCompetitor],
    heuristic: &CompanyOverviewSummary,
    cognee_memory: Option<&str>,
) -> Result<(CompanyOverviewSummary, String)> {
    let fallback = render_markdown_brief(company_name, heuristic, competitors, &discovery.raw_serp_dumps);
    let Some(provider) = config.default_inference_provider() else {
        return Ok((heuristic.clone(), fallback));
    };
    let selection = InferenceSelection {
        provider,
        model: config.configured_or_default_model(provider),
    };
    let system_prompt = "You are a McKinsey-grade competitive intelligence analyst. Write a founder-facing overview using ONLY the provided evidence. Do not invent ratings, competitors, customer sentiment, compliance posture, or market claims. If evidence is thin, say exactly what is missing. Produce polished markdown with concrete tables, ratings, source-backed fault lines, compliance and policy posture, where the company is better or worse, and tactical next moves.";
    let user_prompt = render_synthesis_prompt(
        company_name,
        group_data_text,
        heuristic,
        competitors,
        discovery,
        cognee_memory,
    );
    let generated = timeout(
        Duration::from_secs(10),
        inference::generate_text(config, &selection, system_prompt, &user_prompt),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .map(|text| text.trim().to_owned())
    .filter(|text| text.len() >= 400)
    .unwrap_or(fallback);

    Ok((heuristic.clone(), truncate_markdown(&generated)))
}

async fn fetch_cognee_memory_context(
    state: &AppState,
    company_id: &str,
    company_name: &str,
) -> Option<String> {
    let cognee = state.cognee.as_ref()?;
    let dataset = crate::cognee::dataset_name_for_group(company_id);
    let queries = [
        format!("{company_name} financial performance revenue sales growth profitability"),
        format!("{company_name} competitors customer trust complaints reviews ratings"),
        format!("{company_name} launches risks controversies supply chain hiring"),
    ];
    let mut blocks = Vec::new();
    for query in queries {
        match cognee.search(&query, &dataset, "GRAPH_COMPLETION").await {
            Ok(results) => {
                let snippets = results
                    .into_iter()
                    .map(|item| item.text.trim().to_owned())
                    .filter(|item| !item.is_empty())
                    .take(3)
                    .collect::<Vec<_>>();
                if !snippets.is_empty() {
                    blocks.push(format!("Query: {query}\n{}", snippets.join("\n\n")));
                }
            }
            Err(err) => {
                tracing::debug!(error = ?err, company_id, query, "cognee overview memory lookup failed");
            }
        }
    }
    if blocks.is_empty() {
        None
    } else {
        Some(blocks.join("\n\n"))
    }
}

fn render_synthesis_prompt(
    company_name: &str,
    group_data_text: &str,
    heuristic: &CompanyOverviewSummary,
    competitors: &[OverviewCompetitor],
    discovery: &DiscoveryResult,
    cognee_memory: Option<&str>,
) -> String {
    let company_rating = rating_source_details(&discovery.company_evidence).join("; ");
    let company_compliance = compliance_signal_summary(&discovery.company_evidence);
    let company_evidence = ranked_evidence(&discovery.company_evidence)
        .into_iter()
        .take(8)
        .map(render_evidence_line)
        .collect::<Vec<_>>()
        .join("\n");
    let competitor_blocks = competitors
        .iter()
        .map(|competitor| {
            let evidence = ranked_evidence(&competitor.evidence)
                .into_iter()
                .take(8)
                .map(render_evidence_line)
                .collect::<Vec<_>>()
                .join("\n");
            let compliance = compliance_signal_summary(&competitor.evidence);
            format!(
                "## {}\nClassification: {}\nDomain: {}\nScore: {:.1}\nRating summary: {}\nTrust: {}\nFaults: {}\nCompliance: {}\nBetter/wedge: {}\nDurability: {}\nEvidence:\n{}",
                competitor.name,
                competitor.classification,
                competitor.domain.as_deref().unwrap_or("unknown"),
                competitor.score,
                competitor.rating_summary,
                competitor.customer_trust,
                competitor.faults,
                compliance,
                competitor.where_to_do_better,
                competitor.durability_summary,
                evidence
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "Company: {company_name}\n\nOnboarding context:\n{group_data_text}\n\nCognee memory:\n{}\n\nFirst-pass summary:\n- Actual competitors: {}\n- Customer trust: {}\n- Faults: {}\n- Ratings: {}\n- Where to do better: {}\n- Durability: {}\n- Saturation: {}\n\nCompany rating evidence: {}\nCompany compliance posture: {}\nCompany evidence:\n{}\n\nCompetitor evidence:\n{}\n\nRequired output:\n1. Executive read\n2. Rating table comparing company vs competitors\n3. Customer trust table\n4. Nitty-gritty fault lines with evidence\n5. Compliance and policy posture across the company and competitors\n6. Where our company is doing better and worse\n7. What to ask/research next if evidence is missing\n8. Founder action plan",
        cognee_memory.unwrap_or("No prior Cognee memory retrieved."),
        heuristic.actual_competitors,
        heuristic.customer_trust_and_desire_to_use,
        heuristic.faults,
        heuristic.rating,
        heuristic.where_to_do_better,
        heuristic.how_long_this_will_last,
        heuristic.market_saturation_and_overlap,
        if company_rating.is_empty() { "not extracted".to_owned() } else { company_rating },
        company_compliance,
        company_evidence,
        competitor_blocks
    )
}

fn render_evidence_line(item: &OverviewEvidence) -> String {
    let rating = item
        .rating
        .map(|value| format!(" rating={value:.1}/5"))
        .unwrap_or_default();
    let review_count = item
        .review_count
        .map(|value| format!(" reviews={value}"))
        .unwrap_or_default();
    format!(
        "- [{}] {}{}{} url={} snippet={}",
        item.source_type,
        item.label,
        rating,
        review_count,
        item.url.as_deref().unwrap_or("none"),
        item.snippet.replace('\n', " ")
    )
}

fn build_heuristic_summary(
    company_name: &str,
    discovery: &DiscoveryResult,
    competitors: &[OverviewCompetitor],
) -> CompanyOverviewSummary {
    let actuals = competitors
        .iter()
        .filter(|item| item.classification == "actual")
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    let mentions = competitors
        .iter()
        .filter(|item| item.classification != "actual")
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    let avg_rating = average_competitor_rating(competitors);
    let company_rating_details = rating_source_details(&discovery.company_evidence);
    let company_rating = average_rating(&discovery.company_evidence);
    let company_review_count = total_review_count(&discovery.company_evidence);
    let (company_positive, company_negative) = classify_review_text(&discovery.company_evidence);
    let evidence_backed = competitors
        .iter()
        .filter(|item| has_reddit_or_review_evidence(&item.evidence))
        .collect::<Vec<_>>();
    let source_thin = competitors
        .iter()
        .filter(|item| !has_reddit_or_review_evidence(&item.evidence))
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    let saturated_actuals = actuals.len();
    let review_backed_count = evidence_backed.len();

    CompanyOverviewSummary {
        actual_competitors: if actuals.is_empty() {
            if mentions.is_empty() {
                format!(
                    "No competitor was confirmed with enough public evidence for {company_name}. Search surfaced weak mentions, but none had enough review, Reddit, or overlap evidence to treat as a firm head-to-head rival."
                )
            } else {
                format!(
                    "Strongly mentioned names are {}. They appeared in search or onboarding, but at least part of this set still needs stronger customer-review or comparison evidence before it should be treated as a locked-in head-to-head competitor list.",
                    mentions.join(", ")
                )
            }
        } else if mentions.is_empty() {
            format!(
                "Confirmed head-to-head competitors are {}. They were promoted into the actual set because we found overlap signals across search, official sites, customer-review sources, Reddit, or comparison intent pages.",
                actuals.join(", ")
            )
        } else {
            format!(
                "Confirmed head-to-head competitors are {}. Additional mentioned or adjacent names include {}, but those are weaker because the current run found less customer-proof or direct comparison evidence.",
                actuals.join(", "),
                mentions.join(", ")
            )
        },
        customer_trust_and_desire_to_use: {
            let mut lines = Vec::new();
            lines.push(format!(
                "{}: {}",
                company_name,
                company_customer_standing_summary(
                    company_rating,
                    company_review_count,
                    &discovery.company_evidence,
                    &company_positive,
                    &company_negative,
                )
            ));
            lines.extend(
                competitors
                    .iter()
                    .map(|item| format!("{}: {}", item.name, item.customer_trust)),
            );
            lines.join(" ")
        },
        faults: {
            let mut lines = Vec::new();
            lines.push(format!(
                "{}: {}",
                company_name,
                faults_summary(&company_negative, &discovery.company_evidence)
            ));
            lines.extend(
                competitors
                    .iter()
                    .map(|item| format!("{}: {}", item.name, item.faults)),
            );
            lines.join(" ")
        },
        rating: {
            let company_rating_line = if company_rating_details.is_empty() {
                format!(
                    "{}: {}",
                    company_name,
                    rating_summary(company_rating, company_review_count, &discovery.company_evidence)
                )
            } else {
                format!("{}: {}", company_name, company_rating_details.join("; "))
            };
            let competitor_rating_lines = competitors
                .iter()
                .filter_map(competitor_rating_line)
                .collect::<Vec<_>>();
            if let Some(value) = avg_rating {
                format!(
                    "{} Competitor rating average where extracted: {:.1}/5. Competitor source ratings: {}",
                    company_rating_line,
                    value,
                    if competitor_rating_lines.is_empty() {
                        competitors
                            .iter()
                            .map(|item| format!("{}: {}", item.name, item.rating_summary))
                            .collect::<Vec<_>>()
                            .join(" ")
                    } else {
                        competitor_rating_lines.join(" ")
                    }
                )
            } else {
                format!(
                    "{} Competitor ratings: {}",
                    company_rating_line,
                    competitors
                        .iter()
                        .map(|item| format!("{}: {}", item.name, item.rating_summary))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            }
        },
        where_to_do_better: competitors
            .iter()
            .map(|item| format!("Against {}, {}.", item.name, item.where_to_do_better))
            .collect::<Vec<_>>()
            .join(" "),
        how_long_this_will_last: competitors
            .iter()
            .map(|item| format!("{}: {}", item.name, item.durability_summary))
            .collect::<Vec<_>>()
            .join(" "),
        market_saturation_and_overlap: format!(
            "{} competitors were surfaced, {} were strong enough to classify as actual, and {} had direct customer-review or Reddit evidence.{}",
            competitors.len(),
            saturated_actuals,
            review_backed_count,
            if source_thin.is_empty() {
                " The space looks genuinely competitive rather than populated by one-off search mentions.".to_owned()
            } else {
                format!(
                    " Source coverage is still thin for {}.",
                    source_thin.join(", ")
                )
            }
        ),
        confidence_notes: if discovery.recent_news.is_empty() {
            format!(
                "Recent-news evidence was limited. Confidence is highest on competitors with direct review or Reddit evidence and lower on names that only surfaced through search or onboarding."
            )
        } else {
            format!(
                "Confidence is strongest where search discovery, customer discussion, and recent news all lined up. {} recent news items were captured in this run.",
                discovery.recent_news.len()
            )
        },
    }
}

fn render_markdown_brief(
    company_name: &str,
    summary: &CompanyOverviewSummary,
    competitors: &[OverviewCompetitor],
    _raw_serp_dumps: &[RawSerpDump],
) -> String {
    let actual_count = competitors
        .iter()
        .filter(|item| item.classification == "actual")
        .count();
    let customer_backed_count = competitors
        .iter()
        .filter(|item| has_reddit_or_review_evidence(&item.evidence))
        .count();
    let executive_summary = format!(
        "{company_name} operates in a market where {} competitors were surfaced, {} were confirmed as head-to-head, and {} had customer-review, Reddit, or brand-review search evidence. The strongest read is that customer trust and operational faults need to be evaluated competitor by competitor rather than inferred from brand size alone.",
        competitors.len(),
        actual_count,
        customer_backed_count,
    );
    let competitor_md = competitors
        .iter()
        .map(|competitor| {
            let mut lines = vec![format!("### {}", competitor.name)];
            lines.push(format!("**Role in market:** {}", competitor.classification));
            if let Some(domain) = &competitor.domain {
                lines.push(format!("**Domain:** `{domain}`"));
            }
            lines.push(format!("**Competitive overlap:** {}", competitor.overlap_summary));
            lines.push(format!("**Customer standing:** {}", competitor.customer_trust));
            lines.push(format!("**Fault lines:** {}", competitor.faults));
            lines.push(format!("**Visible ratings:** {}", competitor.rating_summary));
            lines.push(format!(
                "**Compliance posture:** {}",
                compliance_signal_summary(&competitor.evidence)
            ));
            lines.push(format!("**Strategic opening:** {}", competitor.where_to_do_better));
            lines.push(format!("**Durability:** {}", competitor.durability_summary));
            lines.push(format!("**Saturation read:** {}", competitor.saturation_summary));
            if !competitor.evidence.is_empty() {
                lines.push("**Evidence used:**".to_owned());
                for item in ranked_evidence(&competitor.evidence).iter().take(6) {
                    let target = item
                        .url
                        .as_deref()
                        .unwrap_or("source unavailable");
                    lines.push(format!(
                        "- {} ([link]({})) — {}",
                        item.label,
                        target,
                        item.snippet.replace('\n', " ")
                    ));
                }
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "# Competitive Intelligence Overview: {company_name}\n\n## Executive read\n{}\n\n## Market map\n{}\n\n## Customer standing\n{}\n\n## Fault lines\n{}\n\n## Rating and trust signals\n{}\n\n## Compliance and policy posture\n{}\n\n## Strategic openings\n{}\n\n## Durability of the advantage\n{}\n\n## Saturation and overlap\n{}\n\n## Confidence\n{}\n\n## Competitor dossiers\n\n{}",
        executive_summary,
        summary.actual_competitors,
        summary.customer_trust_and_desire_to_use,
        summary.faults,
        summary.rating,
        compliance_signal_summary(
            &competitors
                .iter()
                .flat_map(|competitor| competitor.evidence.iter().cloned())
                .collect::<Vec<_>>()
        ),
        summary.where_to_do_better,
        summary.how_long_this_will_last,
        summary.market_saturation_and_overlap,
        summary.confidence_notes,
        competitor_md
    )
}

async fn set_overview_status(
    db: &Db,
    user_id: ObjectId,
    company_id: ObjectId,
    company_name: &str,
    status: CompanyOverviewStatus,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    failure_reason: Option<&str>,
) -> Result<()> {
    let now = Utc::now();
    db.company_overviews()
        .update_one(
            doc! { "user_id": user_id, "company_id": company_id },
            doc! {
                "$set": {
                    "user_id": user_id,
                    "company_id": company_id,
                    "company_name": company_name,
                    "status": status.as_str(),
                    "started_at": started_at.map(mongodb::bson::DateTime::from_chrono),
                    "completed_at": completed_at.map(mongodb::bson::DateTime::from_chrono),
                    "failure_reason": failure_reason.unwrap_or(""),
                    "updated_at": mongodb::bson::DateTime::from_chrono(now),
                },
                "$setOnInsert": {
                    "created_at": mongodb::bson::DateTime::from_chrono(now),
                    "discovered_competitors": mongodb::bson::to_bson(&Vec::<OverviewCompetitor>::new()).unwrap_or(Bson::Null),
                    "markdown_brief": "",
                }
            },
            mongodb::options::UpdateOptions::builder()
                .upsert(true)
                .build(),
        )
        .await
        .context("failed to update overview status")?;
    Ok(())
}

async fn set_overview_failed(
    db: &Db,
    user_id: ObjectId,
    company_id: ObjectId,
    company_name: Option<&str>,
    failure_reason: &str,
) -> Result<()> {
    set_overview_status(
        db,
        user_id,
        company_id,
        company_name.unwrap_or("Unknown company"),
        CompanyOverviewStatus::Failed,
        Some(Utc::now()),
        Some(Utc::now()),
        Some(failure_reason),
    )
    .await
}

async fn save_completed_overview(db: &Db, doc_value: CompanyOverviewDoc) -> Result<()> {
    let created_at = doc_value.created_at;
    let mut set_doc = mongodb::bson::to_document(&doc_value)
        .context("failed to encode completed overview")?;
    // `$set` and `$setOnInsert` cannot both target `created_at`; the upsert below
    // owns the insert path so we drop it from `$set` and let `$setOnInsert` handle
    // the value for new documents while leaving the existing value alone on updates.
    set_doc.remove("_id");
    set_doc.remove("created_at");

    db.company_overviews()
        .update_one(
            doc! { "user_id": doc_value.user_id, "company_id": doc_value.company_id },
            doc! {
                "$set": set_doc,
                "$setOnInsert": {
                    "created_at": mongodb::bson::DateTime::from_chrono(created_at),
                }
            },
            mongodb::options::UpdateOptions::builder()
                .upsert(true)
                .build(),
        )
        .await
        .context("failed to save completed overview")?;
    Ok(())
}

#[derive(Debug, Clone)]
struct SerpResultItem {
    title: String,
    url: Option<String>,
    snippet: String,
}

#[derive(Debug, Clone)]
struct GoogleSearchResults {
    items: Vec<SerpResultItem>,
}

#[derive(Debug, Clone)]
struct GoogleSearchRequest {
    query: String,
    source_type: String,
    pages: usize,
    extra_params: Option<String>,
}

#[derive(Debug, Clone)]
enum FollowupTarget {
    Company,
    Competitor(String),
}

#[derive(Debug, Clone)]
struct FollowupSearchRequest {
    target: FollowupTarget,
    query: String,
    source_type: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatResearchQuery {
    query: String,
    source_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatToolCall {
    pub label: String,
    pub source_type: String,
    pub query: String,
    pub result_count: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ChatRuntimeContextResult {
    pub prompt_context: Option<String>,
    pub tool_calls: Vec<ChatToolCall>,
}

#[derive(Debug, Clone)]
pub struct ChatCompanyContext {
    pub company_name: String,
    pub data_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolResultItem {
    pub title: String,
    pub url: Option<String>,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatToolOutcome {
    pub source_type: String,
    pub query: String,
    pub items: Vec<ToolResultItem>,
    pub elapsed_ms: u128,
    pub error: Option<String>,
}

impl ChatToolOutcome {
    pub fn result_count(&self) -> usize {
        self.items.len()
    }
}

pub async fn run_chat_tool(
    config: &AppConfig,
    source_type: &str,
    raw_query: &str,
    company_context: Option<&ChatCompanyContext>,
) -> ChatToolOutcome {
    let started = std::time::Instant::now();
    let normalized = normalize_chat_source_type(Some(source_type));
    let query = raw_query.trim().to_owned();

    if normalized == "media" {
        return run_spoken_web_tool(config, &query, started).await;
    }

    if normalized == "triggerware" {
        let items = match crate::triggerware::chat_query(config, &query).await {
            Ok(rows) => rows
                .into_iter()
                .take(8)
                .map(|(title, url, snippet)| ToolResultItem { title, url, snippet })
                .collect::<Vec<_>>(),
            Err(err) => {
                return ChatToolOutcome {
                    source_type: normalized,
                    query,
                    items: Vec::new(),
                    elapsed_ms: started.elapsed().as_millis(),
                    error: Some(err.to_string()),
                };
            }
        };
        return ChatToolOutcome {
            source_type: normalized,
            query,
            items,
            elapsed_ms: started.elapsed().as_millis(),
            error: None,
        };
    }

    let brightdata = match BrightDataConfig::from_app_config(config) {
        Ok(value) => value,
        Err(err) => {
            return ChatToolOutcome {
                source_type: normalized,
                query,
                items: Vec::new(),
                elapsed_ms: started.elapsed().as_millis(),
                error: Some(err.to_string()),
            };
        }
    };

    let client = match reqwest::Client::builder()
        .user_agent("win-win-sentinel/0.1")
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(BRIGHTDATA_HTTP_TIMEOUT_SECS))
        .build()
    {
        Ok(value) => value,
        Err(err) => {
            return ChatToolOutcome {
                source_type: normalized,
                query,
                items: Vec::new(),
                elapsed_ms: started.elapsed().as_millis(),
                error: Some(format!("http client init failed: {err}")),
            };
        }
    };

    let effective_query = marketplace_query_for(&normalized, &query);
    let outcome =
        brightdata_google_search_items(&client, &brightdata, &effective_query, &normalized, 1, None).await;
    let elapsed_ms = started.elapsed().as_millis();
    match outcome {
        Ok(results) => {
            let mut items = results
                .items
                .iter()
                .cloned()
                .take(8)
                .map(|item| ToolResultItem {
                    title: item.title,
                    url: item.url,
                    snippet: item.snippet,
                })
                .collect::<Vec<_>>();

            if should_chat_tool_enrich_with_unlocker(&normalized, &query) {
                let official_urls = chat_tool_official_site_urls(
                    company_context,
                    &results.items,
                    MAX_CHAT_TOOL_OFFICIAL_DOMAINS,
                    MAX_CHAT_TOOL_PAGES_PER_DOMAIN,
                );
                for (label, page_url) in official_urls {
                    let Ok(markdown) =
                        web_unlocker_markdown(&client, &brightdata, &page_url, None).await
                    else {
                        continue;
                    };
                    if markdown.trim().is_empty() {
                        continue;
                    }
                    items.push(ToolResultItem {
                        title: label,
                        url: Some(page_url),
                        snippet: truncate(&markdown, MAX_EVIDENCE_SNIPPET_CHARS),
                    });
                }
            }
            ChatToolOutcome {
                source_type: normalized,
                query,
                items,
                elapsed_ms,
                error: None,
            }
        }
        Err(err) => ChatToolOutcome {
            source_type: normalized,
            query,
            items: Vec::new(),
            elapsed_ms,
            error: Some(err.to_string()),
        },
    }
}

pub fn make_chat_company_context(group_name: &str, data_text: &str) -> ChatCompanyContext {
    ChatCompanyContext {
        company_name: effective_company_name(group_name, &parse_company_seed(group_name, data_text)),
        data_text: data_text.to_owned(),
    }
}

pub fn chat_company_search_subject(context: &ChatCompanyContext) -> String {
    let seed = parse_company_seed(&context.company_name, &context.data_text);
    primary_search_subject(&effective_company_name(&context.company_name, &seed), &seed)
}

pub fn chat_company_query_hints(context: &ChatCompanyContext) -> Vec<String> {
    let seed = parse_company_seed(&context.company_name, &context.data_text);
    seed_query_hints(&seed)
}

fn should_chat_tool_enrich_with_unlocker(source_type: &str, query: &str) -> bool {
    if matches!(source_type, "serp_review" | "news" | "linkedin" | "social") {
        return true;
    }
    let lower = query.to_ascii_lowercase();
    [
        "competitor",
        "compare",
        "comparison",
        "pricing",
        "returns",
        "refund",
        "shipping",
        "privacy",
        "terms",
        "warranty",
        "compliance",
        "legal",
        "sustainability",
        "lawsuit",
        "recall",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn chat_tool_official_site_urls(
    company_context: Option<&ChatCompanyContext>,
    items: &[SerpResultItem],
    max_domains: usize,
    max_pages_per_domain: usize,
) -> Vec<(String, String)> {
    let context_seed = company_context
        .map(|ctx| parse_company_seed(&ctx.company_name, &ctx.data_text));
    let brand_website = context_seed
        .as_ref()
        .and_then(|seed| normalize_website(&seed.website));

    let mut domains = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        if let Some(url) = item.url.as_deref() {
            let Some(host) = host_from_url(url) else {
                continue;
            };
            if should_ignore_competitor_host(&host, brand_website.as_deref())
                || GOOGLE_HOSTS.contains(&host.as_str())
            {
                continue;
            }
            let Some(base) = base_site_url(url) else {
                continue;
            };
            if seen.insert(base.clone()) {
                domains.push(base);
            }
            if domains.len() >= max_domains {
                break;
            }
        }
    }

    domains
        .into_iter()
        .flat_map(|base| official_site_page_targets(&base, max_pages_per_domain))
        .collect()
}

fn base_site_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let scheme = parsed.scheme();
    let host = parsed.host_str()?;
    Some(format!("{scheme}://{host}"))
}

async fn brightdata_serp_json(
    client: &reqwest::Client,
    config: &BrightDataConfig,
    url: &str,
) -> Result<Value> {
    let payload = json!({
        "zone": config.serp_zone,
        "url": url,
        "format": "json",
        "method": "GET",
        "country": "us",
    });
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 1..=BRIGHTDATA_MAX_ATTEMPTS {
        let response = client
            .post("https://api.brightdata.com/request")
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await;

        match response {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if status.is_success() {
                    return serde_json::from_str(&body)
                        .context("failed to parse Bright Data SERP JSON");
                }

                let err = anyhow!("Bright Data SERP API failed with {status}: {}", truncate(&body, 500));
                if is_retryable_brightdata_status(status) && attempt < BRIGHTDATA_MAX_ATTEMPTS {
                    last_error = Some(err);
                    sleep(brightdata_retry_delay(attempt)).await;
                    continue;
                }
                return Err(err);
            }
            Err(err) => {
                let retryable = is_retryable_reqwest_error(&err);
                let err = anyhow!("failed to call Bright Data SERP API: {err}");
                if retryable && attempt < BRIGHTDATA_MAX_ATTEMPTS {
                    last_error = Some(err);
                    sleep(brightdata_retry_delay(attempt)).await;
                    continue;
                }
                return Err(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("Bright Data SERP API failed after retries")))
}

async fn brightdata_serp_raw_markdown(
    client: &reqwest::Client,
    config: &BrightDataConfig,
    url: &str,
) -> Result<String> {
    let payload = json!({
        "zone": config.serp_zone,
        "url": url,
        "format": "raw",
        "method": "GET",
        "country": "us",
        "data_format": "markdown",
    });
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 1..=BRIGHTDATA_MAX_ATTEMPTS {
        let response = client
            .post("https://api.brightdata.com/request")
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await;

        match response {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if status.is_success() {
                    return Ok(extract_brightdata_text_body(&body));
                }

                let err = anyhow!("Bright Data raw SERP failed with {status}: {}", truncate(&body, 500));
                if is_retryable_brightdata_status(status) && attempt < BRIGHTDATA_MAX_ATTEMPTS {
                    last_error = Some(err);
                    sleep(brightdata_retry_delay(attempt)).await;
                    continue;
                }
                return Err(err);
            }
            Err(err) => {
                let retryable = is_retryable_reqwest_error(&err);
                let err = anyhow!("failed to call Bright Data raw SERP: {err}");
                if retryable && attempt < BRIGHTDATA_MAX_ATTEMPTS {
                    last_error = Some(err);
                    sleep(brightdata_retry_delay(attempt)).await;
                    continue;
                }
                return Err(err);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("Bright Data raw SERP failed after retries")))
}

fn extract_brightdata_text_body(body: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return body.to_owned();
    };

    if let Some(text) = value.get("body").and_then(Value::as_str) {
        return text.to_owned();
    }

    if let Some(inner) = value.get("body") {
        return serde_json::to_string_pretty(inner).unwrap_or_else(|_| inner.to_string());
    }

    body.to_owned()
}

async fn brightdata_google_search_items(
    client: &reqwest::Client,
    config: &BrightDataConfig,
    query: &str,
    source_type: &str,
    pages: usize,
    extra_params: Option<&str>,
) -> Result<GoogleSearchResults> {
    let mut out = Vec::new();
    let mut seen_urls = HashSet::new();
    let mut last_error: Option<anyhow::Error> = None;

    for page in 0..pages.max(1) {
        let start = page * 10;
        let mut url = format!(
            "https://www.google.com/search?q={}&start={start}",
            urlencoding::encode(query)
        );
        if let Some(extra_params) = extra_params.filter(|value| !value.trim().is_empty()) {
            url.push('&');
            url.push_str(extra_params);
        }

        match brightdata_serp_raw_markdown(client, config, &url).await {
            Ok(raw_text) => {
                let raw_items = extract_serp_result_items_from_raw_text(&raw_text, source_type);
                for item in raw_items {
                    let dedupe_key = item
                        .url
                        .clone()
                        .unwrap_or_else(|| format!("{}:{}", item.title, item.snippet));
                    if seen_urls.insert(dedupe_key) {
                        out.push(item);
                    }
                }
            }
            Err(err) => {
                last_error = Some(err);
            }
        }
    }

    if out.is_empty() {
        if let Some(err) = last_error {
            tracing::debug!(error = ?err, query, source_type, "Bright Data Google search returned no usable results");
        }
    }

    Ok(GoogleSearchResults {
        items: out,
    })
}

async fn brightdata_google_search_many(
    client: &reqwest::Client,
    config: &BrightDataConfig,
    requests: Vec<GoogleSearchRequest>,
) -> Vec<(String, String, GoogleSearchResults)> {
    futures::stream::iter(requests)
        .map(|request| async move {
            let items = brightdata_google_search_items(
                client,
                config,
                &request.query,
                &request.source_type,
                request.pages,
                request.extra_params.as_deref(),
            )
            .await
            .unwrap_or_else(|_| GoogleSearchResults {
                items: Vec::new(),
            });
            (request.query, request.source_type, items)
        })
        .buffer_unordered(BRIGHTDATA_SERP_QUERY_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
}

fn is_retryable_brightdata_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::BAD_GATEWAY
        || status == StatusCode::SERVICE_UNAVAILABLE
        || status == StatusCode::GATEWAY_TIMEOUT
        || status.is_server_error()
}

fn is_retryable_reqwest_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request()
}

fn brightdata_retry_delay(attempt: usize) -> Duration {
    Duration::from_millis(BRIGHTDATA_RETRY_BASE_DELAY_MS * attempt as u64)
}

fn discovery_search_queries(company_name: &str, seed: &CompanySeed) -> Vec<String> {
    let search_subject = primary_search_subject(company_name, seed);
    let mut queries = vec![
        format!("competitors of {search_subject}"),
        format!("{search_subject} competitors"),
        format!("{search_subject} vs"),
        format!("{search_subject} compared to"),
        format!("{search_subject} rival brands"),
        format!("{search_subject} market competitors"),
        format!("{search_subject} customer reviews competitors"),
        format!("site:reddit.com/r {search_subject} vs"),
        format!("site:reddit.com/r {search_subject} review"),
        format!("site:reddit.com/r {search_subject} complaints"),
        format!("site:trustpilot.com/review {company_name}"),
        format!("site:g2.com/products {company_name} alternatives reviews"),
        format!("site:capterra.com {company_name} alternatives reviews"),
    ];

    if !is_ambiguous_company_name(company_name) || search_subject == company_name {
        queries.extend([
            format!("{company_name} alternatives"),
            format!("best alternatives to {company_name}"),
            format!("{company_name} similar companies"),
            format!("{company_name} complaints alternatives"),
            format!("site:reddit.com/r {company_name} alternatives"),
        ]);
    }

    for hint in seed_query_hints(seed) {
        queries.extend([
            format!("{company_name} {hint} competitors"),
            format!("best {hint} brands"),
            format!("best {hint} companies"),
            format!("top {hint} competitors"),
            format!("{hint} brands like {company_name}"),
            format!("{hint} alternatives to {company_name}"),
            format!("site:reddit.com/r best {hint} brands"),
            format!("site:reddit.com/r {hint} brand complaints"),
            format!("{hint} trustpilot reviews"),
        ]);
    }

    dedupe_queries(queries)
}

fn news_search_queries(company_name: &str, seed: &CompanySeed) -> Vec<String> {
    let mut queries = vec![
        format!("{company_name} competitors OR alternatives"),
        format!("{company_name} launches funding controversy leadership"),
        format!("{company_name} market share competitors"),
    ];
    for hint in seed_query_hints(seed) {
        queries.extend([
            format!("{hint} competitors funding launches"),
            format!("{hint} market share brands"),
        ]);
    }
    dedupe_queries(queries)
}

fn customer_search_queries(
    company_name: &str,
    seed: &CompanySeed,
    competitor: &CandidateAccumulator,
) -> Vec<(&'static str, String)> {
    let name = competitor.name.trim();
    customer_search_queries_for_name(company_name, seed, name, competitor.domain.as_deref())
}

fn customer_search_queries_for_name(
    company_name: &str,
    seed: &CompanySeed,
    name: &str,
    domain: Option<&str>,
) -> Vec<(&'static str, String)> {
    let software_context = is_software_review_context(seed);
    let subject = if name.eq_ignore_ascii_case(company_name) {
        primary_search_subject(name, seed)
    } else {
        name.to_owned()
    };
    let mut queries = vec![
        ("trustpilot", format!("site:trustpilot.com/review {name} reviews")),
        ("trustpilot", format!("{subject} Trustpilot rating reviews complaints")),
        ("reddit", format!("site:reddit.com/r {subject} review OR reviews")),
        ("reddit", format!("site:reddit.com/r {subject} complaints OR problems")),
        ("serp_review", format!("{subject} customer reviews complaints problems")),
        ("serp_review", format!("{subject} product reviews")),
        ("serp_review", format!("don't buy from {subject}")),
        ("serp_review", format!("{subject} BBB complaints reviews")),
        ("serp_review", format!("{subject} privacy policy returns shipping warranty terms")),
        ("serp_review", format!("{subject} lawsuit recall labor sustainability controversy")),
    ];

    if software_context {
        queries.extend([
            ("g2", format!("site:g2.com/products {subject} reviews alternatives")),
            ("capterra", format!("site:capterra.com {subject} reviews alternatives")),
        ]);
    } else {
        queries.extend([
            ("serp_review", format!("{subject} quality complaints")),
            ("serp_review", format!("{subject} sizing complaints")),
        ]);
    }

    if let Some(domain) = domain {
        let root = domain.trim_start_matches("www.");
        queries.extend([
            ("trustpilot", format!("site:trustpilot.com/review {root}")),
            ("trustpilot", format!("site:trustpilot.com/review www.{root}")),
            ("serp_review", format!("{root} reviews complaints")),
            ("serp_review", format!("{root} customer reviews")),
            ("serp_review", format!("{root} product reviews")),
        ]);
    }

    if !name.eq_ignore_ascii_case(company_name) {
        queries.push(("serp_review", format!("{name} vs {company_name} reviews")));
    }

    dedupe_source_queries(queries)
}

fn is_software_review_context(seed: &CompanySeed) -> bool {
    let text = format!(
        "{} {} {} {}",
        seed.company_name, seed.specialty, seed.customers, seed.notes
    )
    .to_ascii_lowercase();
    [
        "software",
        "saas",
        "app",
        "platform",
        "api",
        "crm",
        "cloud",
        "dashboard",
        "workflow",
        "automation",
        "developer",
        "data tool",
        "analytics",
        "b2b",
        "enterprise",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn seed_query_hints(seed: &CompanySeed) -> Vec<String> {
    let mut hints = Vec::new();
    for raw in [&seed.specialty, &seed.customers, &seed.notes] {
        for segment in raw.split(['.', '\n', ';']) {
            let hint = segment.trim();
            if hint.len() >= 8 && hint.len() <= 90 {
                hints.push(hint.to_owned());
            }
        }
    }
    hints.truncate(4);
    dedupe_queries(hints)
}

fn seed_category_anchors(seed: &CompanySeed) -> Vec<String> {
    let mut anchors = Vec::new();
    for raw in [&seed.specialty, &seed.customers, &seed.notes] {
        for part in raw.split([',', ';', '.', '\n', '/', '|']) {
            let cleaned = part
                .trim()
                .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != ' ' && ch != '-');
            if cleaned.len() < 4 || cleaned.len() > 40 {
                continue;
            }
            let lowered = cleaned.to_ascii_lowercase();
            if ["customers", "target markets", "additional notes", "known competitors"]
                .iter()
                .any(|needle| lowered == *needle)
            {
                continue;
            }
            anchors.push(cleaned.to_owned());
        }
    }
    anchors.truncate(6);
    dedupe_queries(anchors)
}

fn is_ambiguous_company_name(company_name: &str) -> bool {
    let normalized = company_name.trim();
    let words = normalized.split_whitespace().collect::<Vec<_>>();
    words.len() == 1
        && normalized.len() <= 8
        && normalized.chars().all(|ch| ch.is_ascii_alphabetic())
}

fn primary_search_subject(company_name: &str, seed: &CompanySeed) -> String {
    if is_ambiguous_company_name(company_name) {
        if let Some(anchor) = seed_category_anchors(seed).into_iter().next() {
            return format!("{company_name} {anchor}");
        }
    }
    company_name.to_owned()
}

fn dedupe_queries(queries: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    queries
        .into_iter()
        .map(|query| query.trim().to_owned())
        .filter(|query| !query.is_empty())
        .filter(|query| seen.insert(query.to_ascii_lowercase()))
        .collect()
}

fn dedupe_source_queries(
    queries: Vec<(&'static str, String)>,
) -> Vec<(&'static str, String)> {
    let mut seen = HashSet::new();
    queries
        .into_iter()
        .map(|(source, query)| (source, query.trim().to_owned()))
        .filter(|(_, query)| !query.is_empty())
        .filter(|(source, query)| seen.insert(format!("{source}:{}", query.to_ascii_lowercase())))
        .collect()
}

async fn web_unlocker_markdown(
    client: &reqwest::Client,
    config: &BrightDataConfig,
    url: &str,
    country: Option<&str>,
) -> Result<String> {
    let mut payload = json!({
        "zone": config.web_unlocker_zone,
        "url": url,
        "format": "raw",
        "data_format": "markdown",
    });
    if let Some(country) = country {
        payload["country"] = Value::String(country.to_owned());
    }
    let mut last_error: Option<anyhow::Error> = None;
    for attempt in 1..=BRIGHTDATA_MAX_ATTEMPTS {
        let response = client
            .post("https://api.brightdata.com/request")
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await;

        match response {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    let err = anyhow!("Bright Data Web Unlocker failed with {status}: {}", truncate(&body, 500));
                    if is_retryable_brightdata_status(status) && attempt < BRIGHTDATA_MAX_ATTEMPTS {
                        last_error = Some(err);
                        sleep(brightdata_retry_delay(attempt)).await;
                        continue;
                    }
                    return Err(err);
                }
                let value: Value = serde_json::from_str(&body).context("failed to parse Web Unlocker response")?;
                let text = value
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                if text.is_empty() {
                    return Err(anyhow!("Bright Data Web Unlocker returned empty content"));
                }
                return Ok(text);
            }
            Err(err) => {
                let retryable = is_retryable_reqwest_error(&err);
                let err = anyhow!("failed to call Bright Data Web Unlocker: {err}");
                if retryable && attempt < BRIGHTDATA_MAX_ATTEMPTS {
                    last_error = Some(err);
                    sleep(brightdata_retry_delay(attempt)).await;
                    continue;
                }
                return Err(err);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("Bright Data Web Unlocker failed after retries")))
}

async fn dataset_scrape(
    client: &reqwest::Client,
    api_key: &str,
    dataset_id: &str,
    url: &str,
) -> Result<Value> {
    let endpoint = format!(
        "https://api.brightdata.com/datasets/v3/scrape?dataset_id={dataset_id}&format=json"
    );
    let payload = format!(r#"[{{"url":"{url}"}}]"#);
    let mut last_error: Option<anyhow::Error> = None;
    for attempt in 1..=BRIGHTDATA_MAX_ATTEMPTS {
        let response = client
            .post(endpoint.clone())
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .body(payload.clone())
            .send()
            .await;

        match response {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    let err = anyhow!("Bright Data dataset scrape failed with {status}: {}", truncate(&body, 500));
                    if is_retryable_brightdata_status(status) && attempt < BRIGHTDATA_MAX_ATTEMPTS {
                        last_error = Some(err);
                        sleep(brightdata_retry_delay(attempt)).await;
                        continue;
                    }
                    return Err(err);
                }
                return serde_json::from_str(&body).context("failed to parse dataset scraper response");
            }
            Err(err) => {
                let retryable = is_retryable_reqwest_error(&err);
                let err = anyhow!("failed to call Bright Data dataset scraper: {err}");
                if retryable && attempt < BRIGHTDATA_MAX_ATTEMPTS {
                    last_error = Some(err);
                    sleep(brightdata_retry_delay(attempt)).await;
                    continue;
                }
                return Err(err);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("Bright Data dataset scraper failed after retries")))
}

fn browser_api_markdown(config: &BrightDataConfig, url: &str) -> Result<String> {
    let Some(auth) = config.scraping_browser_auth.as_deref() else {
        return Err(anyhow!(
            "Bright Data browser fallback was needed but BRIGHTDATA_SCRAPING_BROWSER_AUTH is not configured"
        ));
    };

    let script_path = browser_fetch_script_path()?;
    let output = task::block_in_place(|| {
        Command::new("bun")
            .arg(script_path)
            .arg(url)
            .env("BRIGHTDATA_SCRAPING_BROWSER_AUTH", auth)
            .output()
    })
    .context("failed to execute browser fallback command")?;

    if !output.status.success() {
        return Err(anyhow!(
            "Bright Data browser fallback failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let raw = String::from_utf8(output.stdout).context("browser fallback stdout was not valid utf-8")?;
    let value: Value = serde_json::from_str(raw.trim()).context("failed to parse browser fallback output")?;
    let content = value
        .get("markdown")
        .and_then(Value::as_str)
        .or_else(|| value.get("html").and_then(Value::as_str))
        .unwrap_or_default()
        .trim()
        .to_owned();
    if content.is_empty() {
        return Err(anyhow!("Bright Data browser fallback returned empty content"));
    }
    Ok(content)
}

fn browser_fetch_script_path() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("failed to resolve current directory")?;
    Ok(cwd.join("scripts/brightdata-browser-fetch.mjs"))
}

fn extract_serp_result_items(value: &Value, source_type: &str) -> Vec<SerpResultItem> {
    if let Some(body) = value.get("body") {
        if let Some(text) = body.as_str() {
            if let Ok(inner) = serde_json::from_str::<Value>(text) {
                let inner_items = extract_serp_result_items(&inner, source_type);
                if !inner_items.is_empty() {
                    return inner_items;
                }
            }
            return extract_serp_result_items_from_raw_text(text, source_type);
        }

        let nested_items = extract_serp_result_items(body, source_type);
        if !nested_items.is_empty() {
            return nested_items;
        }
    }

    let mut out = Vec::new();
    for key in [
        "organic",
        "organic_results",
        "news",
        "news_results",
        "top_stories",
        "perspectives",
        "results",
        "items",
    ] {
        if let Some(items) = value.get(key).and_then(Value::as_array) {
            for item in items {
                let title = item
                    .get("title")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("source").and_then(Value::as_str))
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                let url = item
                    .get("link")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("url").and_then(Value::as_str))
                    .map(str::to_owned);
                let snippet = item
                    .get("description")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("snippet").and_then(Value::as_str))
                    .or_else(|| item.get("text").and_then(Value::as_str))
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                if title.is_empty() && url.is_none() && snippet.is_empty() {
                    continue;
                }
                out.push(SerpResultItem {
                    title: if title.is_empty() {
                        format!("{source_type} result")
                    } else {
                        title
                    },
                    url,
                    snippet,
                });
            }
        }
    }
    out
}

fn extract_serp_result_items_from_raw_text(raw: &str, source_type: &str) -> Vec<SerpResultItem> {
    let mut out = Vec::new();
    let lines = raw.lines().collect::<Vec<_>>();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index].trim();
        let Some(title) = line.strip_prefix("### ") else {
            index += 1;
            continue;
        };

        let mut block = Vec::new();
        index += 1;
        while index < lines.len() && !lines[index].trim().starts_with("### ") {
            block.push(lines[index].trim().to_owned());
            index += 1;
        }

        if let Some(item) = serp_item_from_raw_block(title.trim(), &block, source_type) {
            out.push(item);
        }
    }

    out
}

fn serp_item_from_raw_block(
    title: &str,
    block: &[String],
    source_type: &str,
) -> Option<SerpResultItem> {
    if is_noise_serp_title(title) {
        return None;
    }

    let mut url = None;
    let mut snippet_parts = Vec::new();
    let mut seen_url = false;

    for line in block {
        let line = line.trim();
        if line.is_empty() || is_noise_serp_line(line) {
            continue;
        }

        if url.is_none() {
            url = extract_url_from_serp_line(line);
            if url.is_some() {
                seen_url = true;
                continue;
            }
        } else if extract_url_from_serp_line(line).is_some() {
            seen_url = true;
            continue;
        }

        if !seen_url {
            continue;
        }

        if !looks_like_duplicate_source_line(line, url.as_deref()) {
            snippet_parts.push(line.to_owned());
        }
    }

    let snippet = truncate(
        &snippet_parts
            .join(" ")
            .replace("  ", " ")
            .trim()
            .to_owned(),
        MAX_EVIDENCE_SNIPPET_CHARS,
    );

    if title.is_empty() && url.is_none() && snippet.is_empty() {
        return None;
    }

    Some(SerpResultItem {
        title: if title.is_empty() {
            format!("{source_type} result")
        } else {
            title.to_owned()
        },
        url,
        snippet,
    })
}

fn extract_url_from_serp_line(line: &str) -> Option<String> {
    for prefix in ["https://", "http://"] {
        if let Some(start) = line.find(prefix) {
            let tail = &line[start..];
            let end = tail
                .find(|ch: char| ch.is_whitespace() || matches!(ch, ')' | ']' | '"' | '\'' | '<'))
                .unwrap_or(tail.len());
            let url = tail[..end]
                .trim_end_matches(|ch| matches!(ch, ',' | '.' | ';' | ':'))
                .to_owned();
            if url.starts_with(prefix) {
                return Some(url);
            }
        }
    }
    None
}

fn is_noise_serp_title(title: &str) -> bool {
    let normalized = title.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "ai mode"
            | "search results"
            | "people also ask"
            | "people also search for"
            | "related searches"
            | "pagination"
            | "footer links"
            | "navigation"
    )
}

fn is_noise_serp_line(line: &str) -> bool {
    let normalized = line.trim().to_ascii_lowercase();
    normalized.is_empty()
        || normalized == "search results"
        || normalized == "about this result"
        || normalized == "cached"
        || normalized == "similar"
        || normalized.starts_with("!")
        || normalized.starts_with("[](")
        || normalized.starts_with("![")
        || normalized.starts_with("previous")
        || normalized.starts_with("next")
}

fn looks_like_duplicate_source_line(line: &str, url: Option<&str>) -> bool {
    let Some(url) = url else {
        return false;
    };
    let host = host_from_url(url).unwrap_or_default();
    let normalized = line
        .trim()
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    !normalized.contains(' ')
        && !host.is_empty()
        && (host == normalized || host.trim_start_matches("www.") == normalized)
}

fn customer_result_matches_source(source: &str, url: Option<&str>) -> bool {
    let Some(url) = url else {
        return source == "serp_review";
    };
    let host = host_from_url(url).unwrap_or_default();
    match source {
        "trustpilot" => host.ends_with("trustpilot.com"),
        "g2" => host.ends_with("g2.com"),
        "capterra" => host.ends_with("capterra.com"),
        "reddit" => host.ends_with("reddit.com") && url.contains("/r/"),
        "linkedin" => host.ends_with("linkedin.com") && url.contains("/company"),
        "serp_review" => {
            !host.ends_with("google.com")
                && !host.ends_with("linkedin.com")
                && !host.ends_with("facebook.com")
                && !host.ends_with("instagram.com")
                && !host.ends_with("youtube.com")
        }
        _ => true,
    }
}

fn is_customer_intent_result(item: &SerpResultItem) -> bool {
    let text = format!("{} {}", item.title, item.snippet).to_ascii_lowercase();
    [
        "review",
        "reviews",
        "complaint",
        "complaints",
        "problem",
        "problems",
        "don't buy",
        "do not buy",
        "bad experience",
        "negative",
        "worst",
        "lawsuit",
        "controversy",
        "return",
        "refund",
        "shipping",
        "sizing",
        "quality",
        "customer service",
        "trustpilot",
        "reddit",
        "bbb",
        "sitejabber",
        "complaintsboard",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

fn customer_evidence_score(source: &str) -> f64 {
    match source {
        "trustpilot" | "g2" | "capterra" => 1.5,
        "reddit" => 1.25,
        "serp_review" => 0.75,
        _ => 0.5,
    }
}

fn push_raw_serp_dump(
    dumps: &mut Vec<RawSerpDump>,
    query: &str,
    source_type: &str,
    results: &GoogleSearchResults,
) {
    if results.items.is_empty() {
        return;
    }

    dumps.push(RawSerpDump {
        query: query.to_owned(),
        source_type: source_type.to_owned(),
        items: results
            .items
            .iter()
            .map(|item| RawSerpItem {
                title: item.title.clone(),
                url: item.url.clone(),
                snippet: item.snippet.clone(),
            })
            .collect(),
    });
}

fn render_raw_serp_dump(dumps: &[RawSerpDump]) -> String {
    if dumps.is_empty() {
        return "No SERP results were captured.".to_owned();
    }

    dumps
        .iter()
        .map(|dump| {
            let mut lines = vec![
                format!("QUERY [{}]: {}", dump.source_type, dump.query),
                format!("RESULTS: {}", dump.items.len()),
            ];
            for (index, item) in dump.items.iter().enumerate() {
                lines.push(format!("{}. {}", index + 1, item.title));
                if let Some(url) = &item.url {
                    lines.push(format!("   URL: {url}"));
                }
                if !item.snippet.trim().is_empty() {
                    lines.push(format!("   SNIPPET: {}", item.snippet.replace('\n', " ")));
                }
            }
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn write_raw_serp_debug_dump(
    company_id: &str,
    company_name: &str,
    debug_run_id: &str,
    dumps: &[RawSerpDump],
) -> Result<PathBuf> {
    let run_dir = std::env::current_dir()
        .context("failed to resolve current directory")?
        .join(OVERVIEW_DEBUG_DUMP_DIR)
        .join(format!(
            "{}-{}-{}",
            debug_run_id,
            slugify_filename(company_name),
            company_id
        ));
    fs::create_dir_all(&run_dir)
        .with_context(|| format!("failed to create debug dump dir {}", run_dir.display()))?;

    fs::write(
        run_dir.join("00-all-serp-results.txt"),
        render_raw_serp_dump(dumps),
    )
    .context("failed to write combined SERP dump")?;

    for (index, dump) in dumps.iter().enumerate() {
        let filename = format!(
            "{:03}-{}-{}.txt",
            index + 1,
            slugify_filename(&dump.source_type),
            slugify_filename(&dump.query)
        );
        fs::write(run_dir.join(filename), render_single_raw_serp_dump(dump))
            .context("failed to write per-query SERP dump")?;
    }

    Ok(run_dir)
}

fn overview_debug_run_id(company_id: &str, company_name: &str) -> String {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = OVERVIEW_EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in format!("{company_id}:{company_name}:{now_ns}:{sequence}").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!(
        "{}-{:016x}",
        Utc::now().format("%Y%m%d-%H%M%S"),
        hash
    )
}

fn render_single_raw_serp_dump(dump: &RawSerpDump) -> String {
    render_raw_serp_dump(std::slice::from_ref(dump))
}

fn slugify_filename(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    let slug = if slug.is_empty() {
        "unknown".to_owned()
    } else {
        slug
    };

    truncate(&slug, 96).trim_matches('-').to_owned()
}

fn customer_serp_evidence(source: &str, item: SerpResultItem) -> OverviewEvidence {
    let source_type = match source {
        "serp_review" => "customer_search",
        other => other,
    };
    let label = match source {
        "trustpilot" => "Trustpilot search result",
        "g2" => "G2 search result",
        "capterra" => "Capterra search result",
        "reddit" => "Reddit search result",
        "serp_review" => "Customer-review search result",
        _ => "Customer evidence search result",
    };
    let text = format!("{} {}", item.title, item.snippet);
    OverviewEvidence {
        source_type: source_type.to_owned(),
        label: label.to_owned(),
        url: item.url,
        snippet: truncate(&text, MAX_EVIDENCE_SNIPPET_CHARS),
        rating: extract_rating_from_text(&text),
        review_count: extract_review_count_from_text(&text),
        metadata: Map::new(),
    }
}

fn review_targets(
    discovery: &DiscoveryResult,
    candidate: &CandidateAccumulator,
) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    let mut targets = Vec::new();

    if let Some(urls) = discovery.review_page_urls.get(&candidate.name) {
        for (source, url) in urls {
            if seen.insert(format!("{source}:{url}")) {
                targets.push((source.clone(), url.clone()));
            }
        }
    }

    if let Some(domain) = candidate.domain.as_deref() {
        let root = domain.trim_start_matches("www.");
        for trustpilot_url in [
            format!("https://www.trustpilot.com/review/{root}"),
            format!("https://www.trustpilot.com/review/www.{root}"),
        ] {
            if seen.insert(format!("trustpilot:{trustpilot_url}")) {
                targets.push(("trustpilot".to_owned(), trustpilot_url));
            }
        }
    }

    targets
}

fn review_evidence_from_value(source_type: &str, url: &str, value: &Value) -> OverviewEvidence {
    let rating = find_first_number_by_keys(value, &["rating", "score", "overall_score", "stars"]);
    let review_count =
        find_first_u64_by_keys(value, &["reviews_count", "review_count", "total_reviews", "reviewCount"]);
    let snippet = collect_text_snippets(value, 6).join(" ");
    OverviewEvidence {
        source_type: source_type.to_owned(),
        label: format!("{source_type} review scraper"),
        url: Some(url.to_owned()),
        snippet: truncate(&snippet, MAX_EVIDENCE_SNIPPET_CHARS),
        rating,
        review_count,
        metadata: Map::new(),
    }
}

fn website_evidence_from_markdown(label: &str, url: &str, markdown: &str) -> OverviewEvidence {
    let mut metadata = Map::new();
    metadata.insert("page_label".to_owned(), Value::String(label.to_owned()));
    OverviewEvidence {
        source_type: "website".to_owned(),
        label: label.to_owned(),
        url: Some(url.to_owned()),
        snippet: truncate(markdown, MAX_EVIDENCE_SNIPPET_CHARS),
        rating: extract_rating_from_text(markdown),
        review_count: extract_review_count_from_text(markdown),
        metadata,
    }
}

fn review_evidence_from_markdown(source_type: &str, url: &str, markdown: &str) -> OverviewEvidence {
    OverviewEvidence {
        source_type: source_type.to_owned(),
        label: format!("{source_type} review page"),
        url: Some(url.to_owned()),
        snippet: truncate(markdown, MAX_EVIDENCE_SNIPPET_CHARS),
        rating: extract_rating_from_text(markdown),
        review_count: extract_review_count_from_text(markdown),
        metadata: Map::new(),
    }
}

fn reddit_evidence_from_markdown(url: &str, markdown: &str) -> OverviewEvidence {
    OverviewEvidence {
        source_type: "reddit".to_owned(),
        label: "Reddit discussion".to_owned(),
        url: Some(url.to_owned()),
        snippet: truncate(&compact_discussion_snippet(markdown), MAX_EVIDENCE_SNIPPET_CHARS),
        rating: None,
        review_count: None,
        metadata: Map::new(),
    }
}

fn linkedin_evidence_from_value(url: &str, value: &Value) -> OverviewEvidence {
    let snippet = collect_text_snippets(value, 8).join(" ");
    let mut metadata = Map::new();
    if let Some(employees) = find_first_u64_by_keys(value, &["employees_in_linkedin", "employee_count"]) {
        metadata.insert("employees".to_owned(), Value::Number(employees.into()));
    }
    if let Some(company_size) = find_first_string_by_keys(value, &["company_size"]) {
        metadata.insert("company_size".to_owned(), Value::String(company_size));
    }
    OverviewEvidence {
        source_type: "linkedin".to_owned(),
        label: "LinkedIn company data".to_owned(),
        url: Some(url.to_owned()),
        snippet: truncate(&snippet, MAX_EVIDENCE_SNIPPET_CHARS),
        rating: None,
        review_count: None,
        metadata,
    }
}

fn overlap_summary(
    company_name: &str,
    candidate: &CandidateAccumulator,
    evidence: &[OverviewEvidence],
) -> String {
    let pricing = evidence
        .iter()
        .any(|item| item.snippet.to_ascii_lowercase().contains("pricing"));
    let features = evidence
        .iter()
        .any(|item| item.snippet.to_ascii_lowercase().contains("feature"));
    match (pricing, features) {
        (true, true) => format!(
            "{} overlaps with {} on both feature positioning and pricing-oriented acquisition pages.",
            candidate.name, company_name
        ),
        (true, false) => format!(
            "{} appears in comparison and alternative searches with pricing overlap signals against {}.",
            candidate.name, company_name
        ),
        (false, true) => format!(
            "{} appears to compete on feature breadth or category messaging with {}.",
            candidate.name, company_name
        ),
        (false, false) => format!(
            "{} is publicly discussed near {} but the overlap evidence is still lighter than ideal.",
            candidate.name, company_name
        ),
    }
}

fn customer_trust_summary(
    rating: Option<f64>,
    review_count: u64,
    evidence: &[OverviewEvidence],
    positive_signals: &[String],
    negative_signals: &[String],
) -> String {
    let rating_details = rating_source_details(evidence);
    let sources = source_labels(evidence);
    let primary_source_phrase = if sources.is_empty() {
        "public sources".to_owned()
    } else {
        sources.join(", ")
    };

    if !rating_details.is_empty() {
        let score_phrase = if review_count > 0 {
            format!("roughly {} visible reviews", review_count)
        } else {
            "limited visible review volume".to_owned()
        };
        return format!(
            "Customer standing from {}: {}. The combined extracted score is about {:.1}/5 across {}. Customers most often praise {}{}.",
            primary_source_phrase,
            rating_details.join("; "),
            rating.unwrap_or_default(),
            score_phrase,
            join_top_themes(positive_signals, "product quality or value"),
            if negative_signals.is_empty() {
                String::new()
            } else {
                format!(
                    ", while the main complaints center on {}",
                    join_top_themes(negative_signals, "trust or experience friction")
                )
            }
        );
    }

    if has_reddit_or_review_evidence(evidence) {
        let sentiment = match (!positive_signals.is_empty(), !negative_signals.is_empty()) {
            (true, true) => format!(
                "People praise {}, but also complain about {}",
                join_top_themes(positive_signals, "parts of the experience"),
                join_top_themes(negative_signals, "recurring pain points")
            ),
            (true, false) => format!(
                "The public discussion skews positive around {}",
                join_top_themes(positive_signals, "the product")
            ),
            (false, true) => format!(
                "The available public discussion skews negative around {}",
                join_top_themes(negative_signals, "a few recurring pain points")
            ),
            (false, false) => "Customer discussion exists, but the extracted pages did not expose a stable sentiment pattern".to_owned(),
        };
        return format!("Customer standing from {}: {}.", primary_source_phrase, sentiment);
    }

    "No Trustpilot, G2, Capterra, or Reddit customer evidence was extracted for this competitor in the current run, so customer standing is still unconfirmed.".to_owned()
}

fn company_customer_standing_summary(
    rating: Option<f64>,
    review_count: u64,
    evidence: &[OverviewEvidence],
    positive_signals: &[String],
    negative_signals: &[String],
) -> String {
    if evidence.is_empty() {
        return "No public customer-review evidence was extracted for the onboarded company in this run.".to_owned();
    }
    customer_trust_summary(
        rating,
        review_count,
        evidence,
        positive_signals,
        negative_signals,
    )
}

fn faults_summary(negative_signals: &[String], evidence: &[OverviewEvidence]) -> String {
    if !negative_signals.is_empty() {
        return format!(
            "Recurring complaints across the extracted sources point to {}.",
            join_top_themes(negative_signals, "isolated complaints")
        );
    }
    if let Some(snippet) = first_discussion_snippet(evidence, &["reddit", "trustpilot", "g2", "capterra", "customer_search"]) {
        return format!("No clean repeated fault cluster surfaced, but customer discussion repeatedly referenced: {}.", snippet);
    }
    if has_reddit_or_review_evidence(evidence) {
        return "Customer-review sources were found, but they did not expose a repeated complaint strong enough to call a dominant market weakness.".to_owned();
    }
    "No customer-review source was extracted, so there is no source-backed fault pattern to claim yet.".to_owned()
}

fn rating_summary(rating: Option<f64>, review_count: u64, evidence: &[OverviewEvidence]) -> String {
    let details = rating_source_details(evidence);
    if !details.is_empty() {
        return if review_count > 0 {
            format!(
                "{}. Approximate combined visible score: {:.1}/5 across about {} reviews.",
                details.join("; "),
                rating.unwrap_or_default(),
                review_count
            )
        } else {
            format!(
                "{}. Review-count visibility was limited.",
                details.join("; ")
            )
        };
    }
    if has_reddit_or_review_evidence(evidence) {
        return "Review-platform or Reddit evidence was found, but no clean numeric rating was extracted from the captured pages.".to_owned();
    }
    "No public Trustpilot, G2, Capterra, or similar rating was extracted in this run.".to_owned()
}

fn compliance_signal_summary(evidence: &[OverviewEvidence]) -> String {
    let mut policy_pages = evidence
        .iter()
        .filter(|item| item.source_type == "website")
        .filter_map(|item| item.metadata.get("page_label").and_then(Value::as_str))
        .filter(|label| {
            let lower = label.to_ascii_lowercase();
            lower.contains("privacy")
                || lower.contains("terms")
                || lower.contains("returns")
                || lower.contains("shipping")
                || lower.contains("warranty")
                || lower.contains("sustainability")
                || lower.contains("compliance")
                || lower.contains("legal")
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    policy_pages.sort();
    policy_pages.dedup();

    let text = evidence
        .iter()
        .map(|item| item.snippet.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let risks = COMPLIANCE_TERMS
        .iter()
        .filter_map(|(pattern, label)| text.contains(pattern).then_some((*label).to_owned()))
        .collect::<Vec<_>>();
    let risks = {
        let mut deduped = risks;
        deduped.sort();
        deduped.dedup();
        deduped
    };

    match (policy_pages.is_empty(), risks.is_empty()) {
        (false, false) => format!(
            "Official policy pages captured: {}. Public compliance or policy-risk signals mention {}.",
            policy_pages.join(", "),
            risks.join(", ")
        ),
        (false, true) => format!(
            "Official policy pages captured: {}. No obvious public compliance breach signal surfaced in this run.",
            policy_pages.join(", ")
        ),
        (true, false) => format!(
            "No official policy page was captured, but public evidence referenced {}.",
            risks.join(", ")
        ),
        (true, true) => {
            "Compliance posture is still thin: no official policy or legal pages were captured in this run.".to_owned()
        }
    }
}

fn improvement_summary(negative_signals: &[String], evidence: &[OverviewEvidence]) -> String {
    if !negative_signals.is_empty() {
        return format!(
            "the clearest public attack surface is {}. If you can prove a better outcome there, that is a real wedge.",
            join_top_themes(negative_signals, "their weak spots")
        );
    }
    if has_reddit_or_review_evidence(evidence) {
        return "no strong public weakness was consistently exposed in customer sources, so do not assume an obvious wedge without deeper review collection.".to_owned();
    }
    if evidence.iter().any(|item| item.source_type == "website") {
        return "public positioning and category messaging are visible, but customer-source proof is still missing. The first job is to find a sharper, externally validated wedge rather than claim one.".to_owned();
    }
    "the current source set is too thin to name a reliable public wedge.".to_owned()
}

fn durability_summary(news_mentions: usize, evidence: &[OverviewEvidence]) -> String {
    let linkedin_growth = evidence.iter().find_map(|item| {
        item.metadata
            .get("employees")
            .and_then(Value::as_u64)
    });
    match (news_mentions, linkedin_growth) {
        (count, Some(employees)) if count >= 2 && employees >= 500 => {
            format!("This looks durable in the near term: {} recent news mentions plus LinkedIn scale around {} employees.", count, employees)
        }
        (count, Some(_)) if count >= 1 => {
            format!("There is some durability signal: {} recent news mentions plus at least one scale signal on LinkedIn, but not enough to assume long-term dominance.", count)
        }
        (0, Some(_)) => {
            "LinkedIn shows some scale, but this run did not capture recent public-news momentum, so timing confidence is limited.".to_owned()
        }
        _ => "No meaningful recent-news or LinkedIn scale signal was captured, so any durability claim here should be treated as low confidence.".to_owned(),
    }
}

fn compact_discussion_snippet(markdown: &str) -> String {
    markdown
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && line.len() > 24
                && !line.starts_with('#')
                && !line.eq_ignore_ascii_case("reddit")
                && !line.to_ascii_lowercase().contains("share this post")
        })
        .take(10)
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_reddit_or_review_evidence(evidence: &[OverviewEvidence]) -> bool {
    evidence.iter().any(|item| {
        matches!(
            item.source_type.as_str(),
            "trustpilot" | "g2" | "capterra" | "reddit" | "customer_search"
        )
    })
}

fn source_labels(evidence: &[OverviewEvidence]) -> Vec<String> {
    let mut labels = evidence
        .iter()
        .filter_map(|item| match item.source_type.as_str() {
            "trustpilot" => Some("Trustpilot"),
            "g2" => Some("G2"),
            "capterra" => Some("Capterra"),
            "reddit" => Some("Reddit"),
            "customer_search" => Some("customer-review search"),
            "linkedin" => Some("LinkedIn"),
            _ => None,
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    labels.sort();
    labels.dedup();
    labels
}

fn join_top_themes(themes: &[String], fallback: &str) -> String {
    if themes.is_empty() {
        fallback.to_owned()
    } else {
        themes.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
    }
}

fn first_discussion_snippet(
    evidence: &[OverviewEvidence],
    source_types: &[&str],
) -> Option<String> {
    evidence
        .iter()
        .find(|item| source_types.iter().any(|source| item.source_type == *source))
        .map(|item| truncate(&item.snippet.replace('\n', " "), 180))
        .filter(|value| !value.trim().is_empty())
}

fn saturation_summary(classification: &str, score: f64, mentioned: bool) -> String {
    if classification == "actual" && score >= 6.0 {
        "This competitor sits inside the core crowded set rather than the fringe.".to_owned()
    } else if mentioned {
        "This looks more like an adjacent or founder-mentioned name than a fully confirmed head-to-head competitor.".to_owned()
    } else {
        "This name is visible in the market, but the head-to-head saturation signal is moderate rather than overwhelming.".to_owned()
    }
}

fn average_rating(evidence: &[OverviewEvidence]) -> Option<f64> {
    let ratings = evidence.iter().filter_map(|item| item.rating).collect::<Vec<_>>();
    if ratings.is_empty() {
        None
    } else {
        Some(ratings.iter().sum::<f64>() / ratings.len() as f64)
    }
}

fn average_competitor_rating(competitors: &[OverviewCompetitor]) -> Option<f64> {
    let ratings = competitors
        .iter()
        .flat_map(|item| item.evidence.iter().filter_map(|e| e.rating))
        .collect::<Vec<_>>();
    if ratings.is_empty() {
        None
    } else {
        Some(ratings.iter().sum::<f64>() / ratings.len() as f64)
    }
}

fn total_review_count(evidence: &[OverviewEvidence]) -> u64 {
    evidence.iter().filter_map(|item| item.review_count).sum()
}

fn classify_review_text(evidence: &[OverviewEvidence]) -> (Vec<String>, Vec<String>) {
    let mut positive: HashMap<String, usize> = HashMap::new();
    let mut negative: HashMap<String, usize> = HashMap::new();
    for item in evidence {
        let text = item.snippet.to_ascii_lowercase();
        for (pattern, label) in POSITIVE_THEME_PATTERNS {
            if text.contains(pattern) {
                *positive.entry((*label).to_owned()).or_insert(0) += 1;
            }
        }
        for (pattern, label) in NEGATIVE_THEME_PATTERNS {
            if text.contains(pattern) {
                *negative.entry((*label).to_owned()).or_insert(0) += 1;
            }
        }
    }
    (
        ranked_theme_labels(positive),
        ranked_theme_labels(negative),
    )
}

fn ranked_theme_labels(map: HashMap<String, usize>) -> Vec<String> {
    let mut items = map.into_iter().collect::<Vec<_>>();
    items.sort_by(|(left_label, left_count), (right_label, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left_label.cmp(right_label))
    });
    items.into_iter().map(|(label, _)| label).collect()
}

const POSITIVE_THEME_PATTERNS: &[(&str, &str)] = &[
    ("easy to use", "ease of use"),
    ("user friendly", "ease of use"),
    ("comfortable", "comfort"),
    ("quality", "product quality"),
    ("reliable", "reliability"),
    ("well made", "product quality"),
    ("support", "customer support"),
    ("customer service", "customer support"),
    ("value", "value for money"),
    ("worth it", "value for money"),
    ("great price", "value for money"),
    ("good price", "value for money"),
    ("fast shipping", "delivery speed"),
    ("love", "customer enthusiasm"),
    ("great fit", "fit and comfort"),
    ("fits well", "fit and comfort"),
    ("durable", "durability"),
    ("stylish", "style"),
    ("looks great", "style"),
    ("responsive", "customer support"),
    ("excellent service", "customer support"),
    ("true to size", "fit and comfort"),
    ("lightweight", "comfort"),
];

const NEGATIVE_THEME_PATTERNS: &[(&str, &str)] = &[
    ("expensive", "price"),
    ("overpriced", "price"),
    ("pricey", "price"),
    ("bugs", "bugs or reliability"),
    ("buggy", "bugs or reliability"),
    ("slow", "speed"),
    ("poor support", "customer support"),
    ("bad support", "customer support"),
    ("poor customer service", "customer support"),
    ("hard to use", "usability"),
    ("confusing", "usability"),
    ("limited", "limited capability"),
    ("return", "returns or refund friction"),
    ("refund", "returns or refund friction"),
    ("shipping", "delivery or logistics"),
    ("delayed", "delivery or logistics"),
    ("late delivery", "delivery or logistics"),
    ("sizing", "fit or sizing"),
    ("fit", "fit or sizing"),
    ("runs small", "fit or sizing"),
    ("runs big", "fit or sizing"),
    ("too narrow", "fit or sizing"),
    ("quality control", "quality control"),
    ("fell apart", "durability"),
    ("fall apart", "durability"),
    ("wear out", "durability"),
    ("wore out", "durability"),
    ("sole", "durability"),
    ("tear", "durability"),
    ("ripped", "durability"),
    ("not durable", "durability"),
    ("cheap", "product quality"),
    ("defective", "quality control"),
    ("out of stock", "availability"),
    ("fake", "trust"),
    ("counterfeit", "trust"),
    ("scam", "trust"),
    ("avoid", "trust"),
    ("cancelled", "delivery or logistics"),
    ("canceled", "delivery or logistics"),
    ("wrong size", "fit or sizing"),
    ("uncomfortable", "comfort"),
    ("blisters", "comfort"),
];

fn competitor_rating_line(competitor: &OverviewCompetitor) -> Option<String> {
    let details = rating_source_details(&competitor.evidence);
    if details.is_empty() {
        None
    } else {
        Some(format!("{}: {}", competitor.name, details.join("; ")))
    }
}

fn rating_source_details(evidence: &[OverviewEvidence]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut details = Vec::new();
    for item in evidence {
        let Some(rating) = item.rating else {
            continue;
        };
        let label = source_display_name(&item.source_type)
            .unwrap_or(item.label.as_str())
            .to_owned();
        let key = format!("{}:{:.2}", label, rating);
        if !seen.insert(key) {
            continue;
        }
        let detail = if let Some(review_count) = item.review_count {
            format!("{label} {:.1}/5 ({} reviews)", rating, review_count)
        } else {
            format!("{label} {:.1}/5", rating)
        };
        details.push(detail);
    }
    details
}

fn source_display_name(source_type: &str) -> Option<&'static str> {
    match source_type {
        "trustpilot" => Some("Trustpilot"),
        "g2" => Some("G2"),
        "capterra" => Some("Capterra"),
        "reddit" => Some("Reddit"),
        "customer_search" => Some("Customer-review search"),
        "linkedin" => Some("LinkedIn"),
        "website" => Some("Website"),
        "serp" => Some("Search"),
        "news" => Some("News"),
        "onboarding" => Some("Onboarding"),
        _ => None,
    }
}

fn ranked_evidence(evidence: &[OverviewEvidence]) -> Vec<&OverviewEvidence> {
    let mut items = evidence.iter().collect::<Vec<_>>();
    items.sort_by(|left, right| {
        evidence_priority(right)
            .cmp(&evidence_priority(left))
            .then_with(|| right.snippet.len().cmp(&left.snippet.len()))
    });
    items
}

fn evidence_priority(item: &OverviewEvidence) -> u8 {
    match item.source_type.as_str() {
        "trustpilot" | "g2" | "capterra" => 6,
        "reddit" => 5,
        "customer_search" => 4,
        "news" => 3,
        "website" => 2,
        "serp" => 1,
        _ => 0,
    }
}

fn parse_company_seed(group_name: &str, data_text: &str) -> CompanySeed {
    CompanySeed {
        company_name: extract_labeled_value(data_text, "Company name")
            .unwrap_or_else(|| group_name.trim().to_owned()),
        website: extract_labeled_value(data_text, "Website").unwrap_or_default(),
        specialty: extract_labeled_value(data_text, "Specializes in").unwrap_or_default(),
        customers: extract_labeled_value(data_text, "Customers or target markets").unwrap_or_default(),
        known_competitors: extract_labeled_value(data_text, "Known competitors").unwrap_or_default(),
        notes: extract_labeled_value(data_text, "Additional notes").unwrap_or_default(),
    }
}

fn effective_company_name(group_name: &str, seed: &CompanySeed) -> String {
    let candidate = seed.company_name.trim();
    if !candidate.is_empty() && !candidate.eq_ignore_ascii_case("new company") {
        candidate.to_owned()
    } else if !group_name.trim().is_empty() {
        group_name.trim().to_owned()
    } else {
        "New company".to_owned()
    }
}

fn extract_labeled_value(data_text: &str, label: &str) -> Option<String> {
    let prefix = format!("- {label}:");
    data_text.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

fn split_competitor_names(raw: &str) -> Vec<String> {
    raw.split([',', '\n', ';'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn normalize_website(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Some(trimmed.to_owned())
    } else {
        Some(format!("https://{trimmed}"))
    }
}

fn candidate_key(name: Option<&str>, domain: Option<&str>) -> String {
    if let Some(domain) = domain {
        let normalized = normalized_key(domain);
        if !normalized.is_empty() {
            return normalized;
        }
    }
    name.map(normalized_key).unwrap_or_default()
}

fn normalized_key(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn host_from_url(raw: &str) -> Option<String> {
    let parsed = Url::parse(raw).ok()?;
    parsed.host_str().map(|value| value.to_ascii_lowercase())
}

fn infer_result_name(title: &str, host: &str) -> Option<String> {
    let first = title
        .split(['|', '-', '–', '—', ':'])
        .next()
        .unwrap_or(title)
        .trim();
    if !first.is_empty() {
        return Some(first.to_owned());
    }
    Some(host.trim_start_matches("www.").to_owned())
}

fn should_ignore_competitor_host(host: &str, brand_website: Option<&str>) -> bool {
    if FILTERED_COMPETITOR_HOSTS
        .iter()
        .any(|ignored| host.ends_with(ignored))
    {
        return true;
    }
    if let Some(brand) = brand_website.and_then(host_from_url) {
        if host == brand || host.ends_with(&brand) {
            return true;
        }
    }
    false
}

fn company_context_text(seed: &CompanySeed, brand_website: Option<&str>) -> String {
    let mut text = format!(
        "{} {} {} {}",
        seed.company_name, seed.specialty, seed.customers, seed.notes
    );
    if let Some(website) = brand_website {
        text.push(' ');
        text.push_str(website);
    }
    text.to_ascii_lowercase()
}

fn is_apparel_or_footwear_context(seed: &CompanySeed, brand_website: Option<&str>) -> bool {
    let text = company_context_text(seed, brand_website);
    APPAREL_TERMS.iter().any(|needle| text.contains(needle))
}

fn looks_automotive_result(title: &str, snippet: &str, url: Option<&str>) -> bool {
    let mut text = format!("{title} {snippet}").to_ascii_lowercase();
    if let Some(url) = url {
        text.push(' ');
        text.push_str(&url.to_ascii_lowercase());
    }
    AUTOMOTIVE_TERMS.iter().any(|needle| text.contains(needle))
}

fn is_irrelevant_for_company_context(
    seed: &CompanySeed,
    brand_website: Option<&str>,
    title: &str,
    snippet: &str,
    url: Option<&str>,
) -> bool {
    is_apparel_or_footwear_context(seed, brand_website)
        && looks_automotive_result(title, snippet, url)
}

fn is_bad_competitor_candidate_name(name: &str) -> bool {
    let text = name.to_ascii_lowercase();
    text.contains(" vs ")
        || text.contains("comparison")
        || text.contains("competitor")
        || text.contains("alternatives")
        || text.contains("similar companies")
        || text.contains("market")
        || text.contains("[example]")
}

fn is_competitor_listicle_result(title: &str, snippet: &str) -> bool {
    let text = format!("{title} {snippet}").to_ascii_lowercase();
    let title_lower = title.to_ascii_lowercase();
    [
        "competitors in 20",
        "competitors of",
        "top competitors",
        "best competitors",
        "alternatives to",
        "best alternatives",
        "similar companies",
        "company profile",
        "market share",
        "ranking",
        "[example]",
    ]
    .iter()
    .any(|needle| title_lower.contains(needle))
        || (text.contains("people also ask") && text.contains("competitors"))
        || (text.contains("list of") && text.contains("competitors"))
}

fn join_path(base_url: &str, path: &str) -> Option<String> {
    let mut url = Url::parse(base_url).ok()?;
    url.set_path(path);
    url.set_query(None);
    Some(url.to_string())
}

fn official_site_page_targets(base_url: &str, max_pages: usize) -> Vec<(String, String)> {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    let candidates = [
        ("Official homepage", None),
        ("About page", Some("about")),
        ("Pricing page", Some("pricing")),
        ("Reviews page", Some("reviews")),
        ("Returns policy", Some("returns")),
        ("Shipping policy", Some("shipping")),
        ("Privacy policy", Some("privacy-policy")),
        ("Terms and conditions", Some("terms-and-conditions")),
        ("Warranty page", Some("warranty")),
        ("Support page", Some("support")),
        ("Sustainability page", Some("sustainability")),
        ("Compliance or legal page", Some("legal")),
    ];

    for (label, path) in candidates {
        let url = match path {
            Some(path) => join_path(base_url, path),
            None => Some(base_url.to_owned()),
        };
        let Some(url) = url else {
            continue;
        };
        if seen.insert(url.clone()) {
            targets.push((label.to_owned(), url));
        }
        if targets.len() >= max_pages {
            break;
        }
    }

    targets
}

fn extract_rating_from_text(text: &str) -> Option<f64> {
    let lowered = text
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii() { ch } else { ' ' })
        .collect::<String>();
    let bytes = lowered.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }

        let start = index;
        let mut end = index + 1;
        let mut has_decimal = false;
        while end < bytes.len() {
            let ch = bytes[end] as char;
            if ch.is_ascii_digit() {
                end += 1;
            } else if ch == '.' && !has_decimal {
                has_decimal = true;
                end += 1;
            } else {
                break;
            }
        }

        let raw = &lowered[start..end];
        if let Ok(value) = raw.parse::<f64>() {
            if (0.0..=5.0).contains(&value) {
                let context_start = start.saturating_sub(60);
                let context_end = (end + 80).min(lowered.len());
                let context = &lowered[context_start..context_end];
                if context.contains("/5")
                    || context.contains("out of 5")
                    || context.contains("star")
                    || context.contains("trustscore")
                    || context.contains("rating")
                    || context.contains("rated")
                    || context.contains("reviews")
                {
                    return Some(value);
                }
            }
        }
        index = end;
    }
    None
}

fn extract_review_count_from_text(text: &str) -> Option<u64> {
    let lowered = text.to_ascii_lowercase();
    for token in lowered.split_whitespace() {
        if token.contains('.') {
            continue;
        }
        let cleaned = token
            .chars()
            .filter(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if cleaned.is_empty() {
            continue;
        }
        if let Ok(value) = cleaned.parse::<u64>() {
            if value > 5 && lowered.contains("review") {
                return Some(value);
            }
        }
    }
    None
}

fn collect_text_snippets(value: &Value, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    collect_text_snippets_inner(value, &mut out, limit);
    out
}

fn collect_text_snippets_inner(value: &Value, out: &mut Vec<String>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                out.push(truncate(trimmed, 220));
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_text_snippets_inner(item, out, limit);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_text_snippets_inner(item, out, limit);
                if out.len() >= limit {
                    break;
                }
            }
        }
        _ => {}
    }
}

fn find_first_number_by_keys(value: &Value, keys: &[&str]) -> Option<f64> {
    find_first_value_by_keys(value, keys).and_then(|item| {
        item.as_f64().or_else(|| item.as_str().and_then(|text| text.parse::<f64>().ok()))
    })
}

fn find_first_u64_by_keys(value: &Value, keys: &[&str]) -> Option<u64> {
    find_first_value_by_keys(value, keys).and_then(|item| {
        item.as_u64().or_else(|| item.as_str().and_then(|text| {
            text.chars()
                .filter(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .ok()
        }))
    })
}

fn find_first_string_by_keys(value: &Value, keys: &[&str]) -> Option<String> {
    find_first_value_by_keys(value, keys)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn find_first_value_by_keys<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                if keys.iter().any(|needle| key.eq_ignore_ascii_case(needle)) {
                    return Some(item);
                }
                if let Some(found) = find_first_value_by_keys(item, keys) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items
            .iter()
            .find_map(|item| find_first_value_by_keys(item, keys)),
        _ => None,
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_owned()
    } else {
        value.chars().take(max_chars).collect()
    }
}

fn truncate_markdown(value: &str) -> String {
    truncate(value.trim(), MAX_MARKDOWN_CHARS)
}

fn snippet_mentions(snippet: &str, candidate_name: &str) -> bool {
    let left = normalized_key(snippet);
    let right = normalized_key(candidate_name);
    !right.is_empty() && left.contains(&right)
}

fn overview_event_payload<T: Serialize>(payload: &T) -> (u64, Value) {
    let event_id = OVERVIEW_EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_queue_requires_real_context() {
        assert!(!should_queue_company_overview(
            "New company",
            "Sentinel company onboarding context.\n- No onboarding fields were provided yet."
        ));
        assert!(should_queue_company_overview(
            "BrightData",
            "Sentinel company onboarding context.\n- Company name: BrightData"
        ));
    }

    #[test]
    fn normalized_key_collapses_noise() {
        assert_eq!(normalized_key("Bright-Data, Inc."), "bright data inc");
    }
}
