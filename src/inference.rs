use crate::config::{AppConfig, InferenceProvider};
use crate::prompt;
use anyhow::{Context, Result, anyhow, bail};
use futures::StreamExt;
use reqwest::Response;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub role: ChatRole,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct InferenceSelection {
    pub provider: InferenceProvider,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InferenceModelSummary {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub enum InferenceStreamEvent {
    MetaTitle(String),
    TextDelta(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
}

impl ChatRole {
    fn as_vultr_role(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    fn as_gemini_role(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "model",
        }
    }
}

pub async fn stream_text<F, Fut>(
    config: &AppConfig,
    selection: &InferenceSelection,
    turns: &[ChatTurn],
    group_data_text: Option<&str>,
    on_event: F,
) -> Result<()>
where
    F: FnMut(InferenceStreamEvent) -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    stream_text_ex(
        config,
        selection,
        turns,
        group_data_text,
        StreamOptions::default(),
        on_event,
    )
    .await
}

#[derive(Debug, Clone, Copy)]
pub struct StreamOptions {
    pub include_streamed_meta: Option<bool>,
    pub include_tools: bool,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            include_streamed_meta: None,
            include_tools: true,
        }
    }
}

pub async fn stream_text_ex<F, Fut>(
    config: &AppConfig,
    selection: &InferenceSelection,
    turns: &[ChatTurn],
    group_data_text: Option<&str>,
    options: StreamOptions,
    mut on_event: F,
) -> Result<()>
where
    F: FnMut(InferenceStreamEvent) -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let include_streamed_meta = options
        .include_streamed_meta
        .unwrap_or_else(|| turns.len() == 1);
    match selection.provider {
        InferenceProvider::Gemini => {
            stream_gemini(
                config,
                selection,
                turns,
                group_data_text,
                include_streamed_meta,
                options.include_tools,
                &mut on_event,
            )
            .await
        }
        InferenceProvider::Vultr => {
            stream_vultr(
                config,
                selection,
                turns,
                group_data_text,
                include_streamed_meta,
                options.include_tools,
                &mut on_event,
            )
            .await
        }
    }
}

pub async fn list_models(
    config: &AppConfig,
    provider: InferenceProvider,
) -> Result<Vec<InferenceModelSummary>> {
    match provider {
        InferenceProvider::Gemini => list_gemini_models(config).await,
        InferenceProvider::Vultr => list_vultr_models(config).await,
    }
}

pub async fn generate_text(
    config: &AppConfig,
    selection: &InferenceSelection,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    match selection.provider {
        InferenceProvider::Gemini => generate_gemini_text(config, selection, system_prompt, user_prompt).await,
        InferenceProvider::Vultr => generate_vultr_text(config, selection, system_prompt, user_prompt).await,
    }
}

async fn stream_gemini<F, Fut>(
    config: &AppConfig,
    selection: &InferenceSelection,
    turns: &[ChatTurn],
    group_data_text: Option<&str>,
    include_streamed_meta: bool,
    include_tools: bool,
    on_event: &mut F,
) -> Result<()>
where
    F: FnMut(InferenceStreamEvent) -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let api_key = config
        .inference_api_key(InferenceProvider::Gemini)
        .ok_or_else(|| anyhow!("GEMINI_API_KEY is required when using Gemini"))?;

    let mut request_body = json!({
        "contents": turns.iter().map(|turn| {
            json!({
                "role": turn.role.as_gemini_role(),
                "parts": [{ "text": turn.body }],
            })
        }).collect::<Vec<_>>(),
        "generationConfig": {
            "maxOutputTokens": config.inference_max_output_tokens,
        }
    });

    let system_prompt = prompt::chat_response_system_prompt(
        &config.inference_system_prompt,
        group_data_text,
        include_streamed_meta,
        include_tools,
    );
    if !system_prompt.trim().is_empty() {
        request_body["systemInstruction"] = json!({
            "parts": [{ "text": system_prompt }],
        });
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
        selection.model
    );
    let response = reqwest::Client::new()
        .post(url)
        .header("x-goog-api-key", api_key)
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("failed to reach Gemini API")?;

    let response = ensure_success(response, "Gemini").await?;
    let mut stream = response.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut post_processor = StreamPostProcessor::new(include_streamed_meta, turns);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("failed to read Gemini streaming response chunk")?;
        let payloads = decoder.push_chunk(&String::from_utf8_lossy(&chunk));
        for payload in payloads {
            let value: Value =
                serde_json::from_str(&payload).context("failed to parse Gemini stream chunk")?;
            let text = extract_gemini_delta(&value);
            for event in post_processor.push(&text) {
                on_event(event).await?;
            }
        }
    }

    for payload in decoder.finish() {
        let value: Value =
            serde_json::from_str(&payload).context("failed to parse Gemini stream chunk")?;
        let text = extract_gemini_delta(&value);
        for event in post_processor.push(&text) {
            on_event(event).await?;
        }
    }

    for event in post_processor.finish() {
        on_event(event).await?;
    }

    Ok(())
}

async fn stream_vultr<F, Fut>(
    config: &AppConfig,
    selection: &InferenceSelection,
    turns: &[ChatTurn],
    group_data_text: Option<&str>,
    include_streamed_meta: bool,
    include_tools: bool,
    on_event: &mut F,
) -> Result<()>
where
    F: FnMut(InferenceStreamEvent) -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let api_key = config
        .inference_api_key(InferenceProvider::Vultr)
        .ok_or_else(|| anyhow!("VULTR_INFERENCE_API_KEY is required when using Vultr"))?;

    let mut messages = Vec::new();
    let system_prompt = prompt::chat_response_system_prompt(
        &config.inference_system_prompt,
        group_data_text,
        include_streamed_meta,
        include_tools,
    );
    if !system_prompt.trim().is_empty() {
        messages.push(json!({
            "role": "system",
            "content": system_prompt,
        }));
    }
    for turn in turns {
        messages.push(json!({
            "role": turn.role.as_vultr_role(),
            "content": turn.body,
        }));
    }

    let request_body = json!({
        "model": selection.model,
        "messages": messages,
        "max_tokens": config.inference_max_output_tokens,
        "stream": true,
    });

    let response = reqwest::Client::new()
        .post("https://api.vultrinference.com/v1/chat/completions")
        .header("authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("failed to reach Vultr Inference API")?;

    let response = ensure_success(response, "Vultr").await?;
    let mut stream = response.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut post_processor = StreamPostProcessor::new(include_streamed_meta, turns);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("failed to read Vultr streaming response chunk")?;
        let payloads = decoder.push_chunk(&String::from_utf8_lossy(&chunk));
        for payload in payloads {
            if payload == "[DONE]" {
                for event in post_processor.finish() {
                    on_event(event).await?;
                }
                return Ok(());
            }

            let value: Value =
                serde_json::from_str(&payload).context("failed to parse Vultr stream chunk")?;
            let text = extract_vultr_delta(&value);
            for event in post_processor.push(&text) {
                on_event(event).await?;
            }
        }
    }

    for payload in decoder.finish() {
        if payload == "[DONE]" {
            for event in post_processor.finish() {
                on_event(event).await?;
            }
            return Ok(());
        }

        let value: Value =
            serde_json::from_str(&payload).context("failed to parse Vultr stream chunk")?;
        let text = extract_vultr_delta(&value);
        for event in post_processor.push(&text) {
            on_event(event).await?;
        }
    }

    for event in post_processor.finish() {
        on_event(event).await?;
    }

    Ok(())
}

async fn generate_gemini_text(
    config: &AppConfig,
    selection: &InferenceSelection,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let api_key = config
        .inference_api_key(InferenceProvider::Gemini)
        .ok_or_else(|| anyhow!("GEMINI_API_KEY is required when using Gemini"))?;

    let mut request_body = json!({
        "contents": [{
            "role": "user",
            "parts": [{ "text": user_prompt }],
        }],
        "generationConfig": {
            "maxOutputTokens": config.inference_max_output_tokens,
        }
    });
    if !system_prompt.trim().is_empty() {
        request_body["systemInstruction"] = json!({
            "parts": [{ "text": system_prompt }],
        });
    }

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        selection.model
    );
    let response = reqwest::Client::new()
        .post(url)
        .header("x-goog-api-key", api_key)
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("failed to reach Gemini API")?;

    let response = ensure_success(response, "Gemini").await?;
    let value: Value = response
        .json()
        .await
        .context("failed to parse Gemini completion response")?;
    let text = value
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("content"))
        .and_then(|item| item.get("parts"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();

    if text.trim().is_empty() {
        return Err(anyhow!("Gemini returned an empty completion"));
    }

    Ok(text)
}

async fn generate_vultr_text(
    config: &AppConfig,
    selection: &InferenceSelection,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let api_key = config
        .inference_api_key(InferenceProvider::Vultr)
        .ok_or_else(|| anyhow!("VULTR_INFERENCE_API_KEY is required when using Vultr"))?;

    let mut messages = Vec::new();
    if !system_prompt.trim().is_empty() {
        messages.push(json!({
            "role": "system",
            "content": system_prompt,
        }));
    }
    messages.push(json!({
        "role": "user",
        "content": user_prompt,
    }));

    let request_body = json!({
        "model": selection.model,
        "messages": messages,
        "max_tokens": config.inference_max_output_tokens,
        "stream": false,
    });

    let response = reqwest::Client::new()
        .post("https://api.vultrinference.com/v1/chat/completions")
        .header("authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("failed to reach Vultr Inference API")?;

    let response = ensure_success(response, "Vultr").await?;
    let value: Value = response
        .json()
        .await
        .context("failed to parse Vultr completion response")?;
    let text = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("message"))
        .and_then(|item| item.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    if text.trim().is_empty() {
        return Err(anyhow!("Vultr returned an empty completion"));
    }

    Ok(text)
}

async fn ensure_success(response: Response, provider_name: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let body = response.text().await.unwrap_or_default();
    bail!("{provider_name} request failed with {status}: {body}");
}

async fn list_gemini_models(config: &AppConfig) -> Result<Vec<InferenceModelSummary>> {
    let api_key = config
        .inference_api_key(InferenceProvider::Gemini)
        .ok_or_else(|| anyhow!("GEMINI_API_KEY is required when using Gemini"))?;
    let client = reqwest::Client::new();
    let mut next_page_token: Option<String> = None;
    let mut seen = HashSet::new();
    let mut models = Vec::new();

    loop {
        let mut request = client
            .get("https://generativelanguage.googleapis.com/v1beta/models")
            .header("x-goog-api-key", api_key)
            .query(&[("pageSize", "1000")]);
        if let Some(token) = &next_page_token {
            request = request.query(&[("pageToken", token.as_str())]);
        }

        let response = request
            .send()
            .await
            .context("failed to reach Gemini API for model listing")?;
        let response = ensure_success(response, "Gemini").await?;
        let value: Value = response
            .json()
            .await
            .context("failed to parse Gemini model list response")?;

        if let Some(items) = value.get("models").and_then(Value::as_array) {
            for item in items {
                let supports_generate = item
                    .get("supportedGenerationMethods")
                    .and_then(Value::as_array)
                    .map(|methods| {
                        methods
                            .iter()
                            .any(|method| method.as_str() == Some("generateContent"))
                    })
                    .unwrap_or(false);
                if !supports_generate {
                    continue;
                }

                let id = item
                    .get("baseModelId")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_owned)
                    .or_else(|| {
                        item.get("name")
                            .and_then(Value::as_str)
                            .map(|name| name.trim_start_matches("models/").to_owned())
                    });

                let Some(id) = id else {
                    continue;
                };

                if !seen.insert(id.clone()) {
                    continue;
                }

                let label = item
                    .get("displayName")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or(&id)
                    .to_owned();

                models.push(InferenceModelSummary { id, label });
            }
        }

        next_page_token = value
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned);

        if next_page_token.is_none() {
            break;
        }
    }

    models.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(models)
}

async fn list_vultr_models(config: &AppConfig) -> Result<Vec<InferenceModelSummary>> {
    let api_key = config
        .inference_api_key(InferenceProvider::Vultr)
        .ok_or_else(|| anyhow!("VULTR_INFERENCE_API_KEY is required when using Vultr"))?;
    let response = reqwest::Client::new()
        .get("https://api.vultrinference.com/v1/models")
        .header("authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .context("failed to reach Vultr Inference API for model listing")?;
    let response = ensure_success(response, "Vultr").await?;
    let value: Value = response
        .json()
        .await
        .context("failed to parse Vultr model list response")?;

    let mut models = value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = item.get("id").and_then(Value::as_str)?.trim();
            if id.is_empty() {
                return None;
            }

            let supports_text_generation = item
                .get("features")
                .and_then(Value::as_array)
                .map(|features| {
                    features
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|feature| feature.eq_ignore_ascii_case("TextGeneration"))
                })
                .unwrap_or(true);
            if !supports_text_generation {
                return None;
            }

            Some(InferenceModelSummary {
                id: id.to_owned(),
                label: id.to_owned(),
            })
        })
        .collect::<Vec<_>>();

    models.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(models)
}

#[derive(Default)]
struct SseDecoder {
    buffer: String,
    data_lines: Vec<String>,
}

struct StreamPostProcessor<'a> {
    user_message: &'a str,
    pending: String,
    meta_handled: bool,
}

impl<'a> StreamPostProcessor<'a> {
    fn new(wants_streamed_meta: bool, turns: &'a [ChatTurn]) -> Self {
        let user_message = turns
            .first()
            .map(|turn| turn.body.as_str())
            .unwrap_or_default();

        Self {
            user_message,
            pending: String::new(),
            meta_handled: !wants_streamed_meta,
        }
    }

    fn push(&mut self, chunk: &str) -> Vec<InferenceStreamEvent> {
        if chunk.is_empty() {
            return Vec::new();
        }

        if self.meta_handled {
            return vec![InferenceStreamEvent::TextDelta(chunk.to_owned())];
        }

        self.pending.push_str(chunk);
        self.drain(false)
    }

    fn finish(&mut self) -> Vec<InferenceStreamEvent> {
        self.drain(true)
    }

    fn drain(&mut self, eof: bool) -> Vec<InferenceStreamEvent> {
        if self.meta_handled {
            if self.pending.is_empty() {
                return Vec::new();
            }

            let text = std::mem::take(&mut self.pending);
            return vec![InferenceStreamEvent::TextDelta(text)];
        }

        let trimmed = self.pending.trim_start_matches(char::is_whitespace);
        if trimmed.is_empty() {
            if eof {
                self.meta_handled = true;
            }
            return Vec::new();
        }

        if !"<meta>".starts_with(trimmed) && !trimmed.starts_with("<meta>") {
            self.meta_handled = true;
            return vec![InferenceStreamEvent::TextDelta(std::mem::take(
                &mut self.pending,
            ))];
        }

        if !trimmed.starts_with("<meta>") {
            if eof {
                self.meta_handled = true;
                return vec![InferenceStreamEvent::TextDelta(std::mem::take(
                    &mut self.pending,
                ))];
            }
            return Vec::new();
        }

        let start_offset = self.pending.len() - trimmed.len();
        let meta_start = start_offset + "<meta>".len();
        let Some(relative_end) = self.pending[meta_start..].find("</meta>") else {
            if eof {
                self.meta_handled = true;
                return vec![InferenceStreamEvent::TextDelta(std::mem::take(
                    &mut self.pending,
                ))];
            }
            return Vec::new();
        };

        let meta_end = meta_start + relative_end;
        let raw_meta = self.pending[meta_start..meta_end].to_owned();
        let remaining = self.pending[meta_end + "</meta>".len()..].to_owned();
        self.pending.clear();
        self.meta_handled = true;

        let mut events = Vec::new();
        if let Some(title) = prompt::accept_streamed_meta_title(&raw_meta, self.user_message) {
            events.push(InferenceStreamEvent::MetaTitle(title));
        }

        let visible = remaining.trim_start_matches(['\r', '\n']);
        if !visible.is_empty() {
            events.push(InferenceStreamEvent::TextDelta(visible.to_owned()));
        }

        events
    }
}

impl SseDecoder {
    fn push_chunk(&mut self, chunk: &str) -> Vec<String> {
        self.buffer.push_str(chunk);
        let mut payloads = Vec::new();

        while let Some(newline_index) = self.buffer.find('\n') {
            let mut line = self.buffer[..newline_index].to_owned();
            self.buffer.drain(..=newline_index);

            if line.ends_with('\r') {
                line.pop();
            }

            if line.is_empty() {
                if !self.data_lines.is_empty() {
                    payloads.push(self.data_lines.join("\n"));
                    self.data_lines.clear();
                }
                continue;
            }

            if let Some(data) = line.strip_prefix("data:") {
                self.data_lines.push(data.trim_start().to_owned());
            }
        }

        payloads
    }

    fn finish(mut self) -> Vec<String> {
        if !self.buffer.trim().is_empty() {
            let line = self.buffer.trim_end_matches('\r');
            if let Some(data) = line.strip_prefix("data:") {
                self.data_lines.push(data.trim_start().to_owned());
            }
        }

        if self.data_lines.is_empty() {
            Vec::new()
        } else {
            vec![self.data_lines.join("\n")]
        }
    }
}

fn extract_gemini_delta(value: &Value) -> String {
    let mut out = String::new();
    let Some(candidates) = value.get("candidates").and_then(Value::as_array) else {
        return out;
    };

    for candidate in candidates {
        let Some(parts) = candidate
            .get("content")
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
        else {
            continue;
        };

        for part in parts {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }
    }

    out
}

fn extract_vultr_delta(value: &Value) -> String {
    let mut out = String::new();
    let Some(choices) = value.get("choices").and_then(Value::as_array) else {
        return out;
    };

    for choice in choices {
        if let Some(text) = choice
            .get("delta")
            .and_then(|delta| delta.get("content"))
            .and_then(Value::as_str)
        {
            out.push_str(text);
        }
    }

    out
}
