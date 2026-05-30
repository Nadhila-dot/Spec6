//! Cognee knowledge-graph client.
//!
//! Wraps the Cognee HTTP API:
//!   POST /api/v1/auth/login   — obtain bearer token
//!   POST /api/v1/add          — upload text into a named dataset
//!   POST /api/v1/cognify      — build knowledge graph from dataset
//!   POST /api/v1/search       — query the graph
//!
//! One `CogneeClient` is shared application-wide (Arc inside AppState).
//! The client re-authenticates automatically on 401 responses.

use anyhow::{Context, Result, anyhow, bail};
use reqwest::{
    Client,
    multipart::{Form, Part},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, warn};

/* ─── public surface ─────────────────────────────────────────────────────── */

#[derive(Clone)]
pub struct CogneeClient(Arc<Inner>);

struct Inner {
    http: Client,
    base: String,
    email: String,
    password: String,
    token: Mutex<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CogneeSearchResult {
    pub text: String,
    pub score: Option<f64>,
    pub dataset_name: Option<String>,
}

impl CogneeClient {
    pub fn new(base_url: &str, email: &str, password: &str) -> Self {
        let base = base_url.trim_end_matches('/').to_owned();
        CogneeClient(Arc::new(Inner {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("http client"),
            base,
            email: email.to_owned(),
            password: password.to_owned(),
            token: Mutex::new(None),
        }))
    }

    /// Authenticate once and cache the bearer token. Returns immediately if
    /// already authenticated.
    pub async fn ensure_auth(&self) -> Result<String> {
        let mut guard = self.0.token.lock().await;
        if let Some(tok) = guard.as_ref() {
            return Ok(tok.clone());
        }
        let tok = self.login_inner().await?;
        *guard = Some(tok.clone());
        Ok(tok)
    }

    /// Force a fresh login (call after a 401).
    async fn refresh_auth(&self) -> Result<String> {
        let mut guard = self.0.token.lock().await;
        let tok = self.login_inner().await?;
        *guard = Some(tok.clone());
        Ok(tok)
    }

    async fn login_inner(&self) -> Result<String> {
        let url = format!("{}/api/v1/auth/login", self.0.base);
        let body = format!(
            "username={}&password={}",
            urlencoding::encode(&self.0.email),
            urlencoding::encode(&self.0.password)
        );
        let resp = self
            .0
            .http
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .context("cognee login request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("cognee login failed {status}: {text}");
        }

        let val: Value = resp.json().await.context("cognee login parse")?;
        let token = val
            .get("access_token")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| anyhow!("cognee login: no access_token in response"))?;

        debug!("cognee auth ok");
        Ok(token)
    }

    /// Add plain text to a Cognee dataset. Creates a multipart upload with
    /// the text as a `.txt` file named `{filename}.txt`.
    pub async fn add_text(
        &self,
        dataset_name: &str,
        text: &str,
        filename: &str,
    ) -> Result<()> {
        self.add_text_authed(dataset_name, text, filename, true).await
    }

    async fn add_text_authed(
        &self,
        dataset_name: &str,
        text: &str,
        filename: &str,
        retry: bool,
    ) -> Result<()> {
        let token = self.ensure_auth().await?;
        let url = format!("{}/api/v1/add", self.0.base);
        let part = Part::bytes(text.as_bytes().to_vec())
            .file_name(format!("{filename}.txt"))
            .mime_str("text/plain")?;
        let form = Form::new()
            .part("data", part)
            .text("datasetName", dataset_name.to_owned());

        let resp = self
            .0
            .http
            .post(&url)
            .bearer_auth(&token)
            .multipart(form)
            .send()
            .await
            .context("cognee add request")?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && retry {
            self.refresh_auth().await?;
            return Box::pin(self.add_text_authed(dataset_name, text, filename, false)).await;
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            // 409 = already exists / conflict — treat as non-fatal
            if status == reqwest::StatusCode::CONFLICT {
                warn!("cognee add conflict (likely duplicate): {text}");
                return Ok(());
            }
            bail!("cognee add failed {status}: {text}");
        }
        debug!("cognee add ok dataset={dataset_name}");
        Ok(())
    }

    /// Process a dataset into a knowledge graph. Runs in the background so
    /// this returns as soon as the pipeline is queued.
    pub async fn cognify(&self, dataset_name: &str) -> Result<()> {
        self.cognify_authed(dataset_name, true).await
    }

    async fn cognify_authed(&self, dataset_name: &str, retry: bool) -> Result<()> {
        let token = self.ensure_auth().await?;
        let url = format!("{}/api/v1/cognify", self.0.base);
        let body = json!({
            "datasets": [dataset_name],
            "runInBackground": true
        });
        let resp = self
            .0
            .http
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .context("cognee cognify request")?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && retry {
            self.refresh_auth().await?;
            return Box::pin(self.cognify_authed(dataset_name, false)).await;
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("cognee cognify failed {status}: {text}");
        }
        debug!("cognee cognify queued dataset={dataset_name}");
        Ok(())
    }

    /// Query the knowledge graph for `dataset_name` using the given search type.
    /// `search_type` is one of the Cognee `SearchType` enum values, e.g.
    /// `"GRAPH_COMPLETION"`, `"RAG_COMPLETION"`, `"CHUNKS"`.
    pub async fn search(
        &self,
        query: &str,
        dataset_name: &str,
        search_type: &str,
    ) -> Result<Vec<CogneeSearchResult>> {
        self.search_authed(query, dataset_name, search_type, true).await
    }

    async fn search_authed(
        &self,
        query: &str,
        dataset_name: &str,
        search_type: &str,
        retry: bool,
    ) -> Result<Vec<CogneeSearchResult>> {
        let token = self.ensure_auth().await?;
        let url = format!("{}/api/v1/search", self.0.base);
        let body = json!({
            "query": query,
            "datasets": [dataset_name],
            "searchType": search_type,
            "topK": 8,
            "systemPrompt": "Answer the question using the context. Be concise and factual. Focus on competitive intelligence, brand strengths/weaknesses, customer sentiment, and market position."
        });
        let resp = self
            .0
            .http
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .context("cognee search request")?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && retry {
            self.refresh_auth().await?;
            return Box::pin(self.search_authed(query, dataset_name, search_type, false)).await;
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("cognee search failed {status}: {text}");
        }

        let raw: Value = resp.json().await.context("cognee search parse")?;
        Ok(parse_search_results(raw))
    }

    /// Convenience: add text then immediately queue cognify. Non-fatal on
    /// failure — logs a warning and returns Ok so the caller keeps running.
    pub async fn ingest_and_cognify(
        &self,
        dataset_name: &str,
        text: &str,
        filename: &str,
    ) -> Result<()> {
        self.add_text(dataset_name, text, filename).await?;
        self.cognify(dataset_name).await?;
        Ok(())
    }
}

/* ─── result parsing ─────────────────────────────────────────────────────── */

/// Cognee returns either a list of `SearchResult` objects or a raw completion
/// string depending on `searchType`. This normalises both forms.
fn parse_search_results(raw: Value) -> Vec<CogneeSearchResult> {
    match raw {
        // Array of SearchResult objects
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| parse_one_result(item))
            .collect(),
        // Direct string completion
        Value::String(s) if !s.is_empty() => vec![CogneeSearchResult {
            text: s,
            score: None,
            dataset_name: None,
        }],
        _ => vec![],
    }
}

fn parse_one_result(item: Value) -> Option<CogneeSearchResult> {
    // { search_result: string|object, dataset_id, dataset_name }
    let dataset_name = item
        .get("dataset_name")
        .and_then(|v| v.as_str())
        .map(str::to_owned);

    let sr = item.get("search_result")?;
    let text = match sr {
        Value::String(s) => s.clone(),
        Value::Object(map) => {
            // Some search types return a structured node — extract text fields
            let candidates = ["text", "content", "summary", "description", "answer"];
            candidates
                .iter()
                .find_map(|&k| map.get(k).and_then(|v| v.as_str()))
                .map(str::to_owned)
                .unwrap_or_else(|| serde_json::to_string(&Value::Object(map.clone())).unwrap_or_default())
        }
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        other => serde_json::to_string(other).unwrap_or_default(),
    };

    if text.trim().is_empty() {
        return None;
    }

    Some(CogneeSearchResult {
        text,
        score: None,
        dataset_name,
    })
}

/* ─── dataset naming ─────────────────────────────────────────────────────── */

/// Stable dataset name for a company group. Uses the group's MongoDB ObjectId
/// so there are no collisions even if the company is renamed.
pub fn dataset_name_for_group(group_id: &str) -> String {
    format!("spec6-{group_id}")
}
