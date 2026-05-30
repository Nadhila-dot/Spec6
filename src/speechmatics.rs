//! Speechmatics realtime speech-to-text proxy.
//!
//! The browser streams raw PCM16 mic audio to our backend over a WebSocket; we
//! relay it to Speechmatics' realtime endpoint and normalise the transcript
//! events back to the frontend. The Speechmatics API key never leaves the
//! server — the browser only ever talks to us.
//!
//! Flow:
//!   1. Exchange the long-lived API key for a short-lived realtime JWT.
//!   2. Open a WS to `wss://…/v2?jwt=…`.
//!   3. Send `StartRecognition` (raw pcm_s16le @ 16 kHz, partials on).
//!   4. Forward binary audio frames; track a seq counter for `EndOfStream`.
//!   5. Map `AddPartialTranscript` / `AddTranscript` to partial/final events.

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

pub const DEFAULT_RT_URL: &str = "wss://eu2.rt.speechmatics.com/v2";
pub const DEFAULT_MGMT_URL: &str = "https://mp.speechmatics.com/v1";
pub const DEFAULT_TTS_URL: &str = "https://preview.tts.speechmatics.com";
pub const DEFAULT_TTS_VOICE: &str = "megan";
pub const DEFAULT_BATCH_URL: &str = "https://asr.api.speechmatics.com/v2";
pub const SAMPLE_RATE: u32 = 16_000;

/// One spoken excerpt pulled from a media transcript.
#[derive(Debug, Clone)]
pub struct TranscriptSegment {
    /// Start offset in seconds.
    pub start: f64,
    pub text: String,
}

impl TranscriptSegment {
    /// "mm:ss" stamp for citing the moment in the source.
    pub fn timestamp(&self) -> String {
        let total = self.start.max(0.0) as u64;
        format!("{}:{:02}", total / 60, total % 60)
    }
}

/// Batch-transcribe audio/video reachable at `media_url`.
///
/// Submits a fetch-URL job to Speechmatics, polls until done, and returns the
/// full transcript plus timestamped segments. This is the engine of the
/// "Spoken Web" scout: it turns earnings calls, YouTube reviews, and podcasts —
/// content text scrapers are blind to — into citable evidence.
pub async fn transcribe_url(
    api_key: &str,
    batch_url: &str,
    media_url: &str,
    max_wait_secs: u64,
) -> Result<(String, Vec<TranscriptSegment>)> {
    let base = batch_url.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("speechmatics batch client build failed")?;

    let config = json!({
        "type": "transcription",
        "transcription_config": { "language": "en", "operating_point": "enhanced" },
        "fetch_data": { "url": media_url },
    });

    // multipart: only the JSON config (audio comes via fetch_data.url).
    let form = reqwest::multipart::Form::new().text("config", config.to_string());
    let submit = client
        .post(format!("{base}/jobs"))
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .context("speechmatics batch submit failed")?;
    if !submit.status().is_success() {
        let status = submit.status();
        let body = submit.text().await.unwrap_or_default();
        bail!("speechmatics batch submit failed {status}: {body}");
    }
    let submit_json: Value = submit.json().await.context("speechmatics submit parse")?;
    let job_id = submit_json
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("speechmatics submit response missing job id"))?
        .to_owned();

    // Poll the job until it leaves the "running" state.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(max_wait_secs);
    loop {
        if std::time::Instant::now() > deadline {
            bail!("speechmatics transcription timed out after {max_wait_secs}s");
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let status_resp = client
            .get(format!("{base}/jobs/{job_id}"))
            .bearer_auth(api_key)
            .send()
            .await
            .context("speechmatics job status failed")?;
        let status_json: Value = status_resp.json().await.context("speechmatics status parse")?;
        let state = status_json
            .get("job")
            .and_then(|j| j.get("status"))
            .and_then(Value::as_str)
            .unwrap_or("running");
        match state {
            "done" => break,
            "running" => continue,
            other => bail!("speechmatics transcription job {other}"),
        }
    }

    // Fetch the JSON transcript and fold it into timestamped segments.
    let transcript_resp = client
        .get(format!("{base}/jobs/{job_id}/transcript?format=json-v2"))
        .bearer_auth(api_key)
        .send()
        .await
        .context("speechmatics transcript fetch failed")?;
    let transcript_json: Value = transcript_resp
        .json()
        .await
        .context("speechmatics transcript parse")?;

    Ok(fold_transcript(&transcript_json))
}

/// Collapse Speechmatics word-level results into readable sentence segments,
/// each carrying the start time of its first word.
fn fold_transcript(value: &Value) -> (String, Vec<TranscriptSegment>) {
    let words = value
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut full = String::new();
    let mut segments: Vec<TranscriptSegment> = Vec::new();
    let mut seg_start: Option<f64> = None;
    let mut seg_text = String::new();

    for item in &words {
        let alt = item
            .get("alternatives")
            .and_then(Value::as_array)
            .and_then(|a| a.first());
        let Some(content) = alt.and_then(|a| a.get("content")).and_then(Value::as_str) else {
            continue;
        };
        let is_punct = item.get("type").and_then(Value::as_str) == Some("punctuation");
        let start = item.get("start_time").and_then(Value::as_f64).unwrap_or(0.0);

        if seg_start.is_none() && !is_punct {
            seg_start = Some(start);
        }
        if is_punct {
            seg_text.push_str(content);
        } else {
            if !seg_text.is_empty() {
                seg_text.push(' ');
            }
            seg_text.push_str(content);
        }
        full.push_str(content);
        full.push(' ');

        // Break a segment on sentence-ending punctuation once it has heft.
        if is_punct && matches!(content, "." | "!" | "?") && seg_text.split_whitespace().count() >= 6
        {
            segments.push(TranscriptSegment {
                start: seg_start.unwrap_or(start),
                text: seg_text.trim().to_owned(),
            });
            seg_text.clear();
            seg_start = None;
        }
    }
    if seg_text.split_whitespace().count() >= 4 {
        segments.push(TranscriptSegment {
            start: seg_start.unwrap_or(0.0),
            text: seg_text.trim().to_owned(),
        });
    }

    (full.trim().to_owned(), segments)
}

/// Exchange a long-lived API key for a short-lived realtime JWT (TTL 1h).
pub async fn fetch_temporary_key(api_key: &str, mgmt_url: &str) -> Result<String> {
    let url = format!("{}/api_keys?type=rt", mgmt_url.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(&url)
        .bearer_auth(api_key)
        .json(&json!({ "ttl": 3600 }))
        .send()
        .await
        .context("speechmatics temporary-key request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("speechmatics temporary-key failed {status}: {body}");
    }

    let value: Value = resp.json().await.context("speechmatics temp-key parse")?;
    value
        .get("key_value")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("speechmatics temp-key response missing key_value"))
}

/// Connect to the Speechmatics realtime endpoint with a temporary JWT.
pub async fn connect_realtime(
    rt_url: &str,
    jwt: &str,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
    let url = format!(
        "{}?jwt={}",
        rt_url.trim_end_matches('/'),
        urlencoding::encode(jwt)
    );
    let (ws, _resp) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .context("speechmatics realtime connect failed")?;
    Ok(ws)
}

pub fn start_recognition_message() -> String {
    json!({
        "message": "StartRecognition",
        "audio_format": {
            "type": "raw",
            "encoding": "pcm_s16le",
            "sample_rate": SAMPLE_RATE,
        },
        "transcription_config": {
            "language": "en",
            "operating_point": "enhanced",
            "enable_partials": true,
            "max_delay": 1.5,
        },
    })
    .to_string()
}

pub fn end_of_stream_message(last_seq_no: u64) -> String {
    json!({ "message": "EndOfStream", "last_seq_no": last_seq_no }).to_string()
}

#[derive(Debug, Clone)]
pub enum SpeechmaticsEvent {
    Started,
    Partial(String),
    Final(String),
    EndOfTranscript,
    Error(String),
    Other,
}

pub fn parse_server_message(text: &str) -> SpeechmaticsEvent {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return SpeechmaticsEvent::Other;
    };
    match value.get("message").and_then(Value::as_str).unwrap_or("") {
        "RecognitionStarted" => SpeechmaticsEvent::Started,
        "AddPartialTranscript" => SpeechmaticsEvent::Partial(transcript_of(&value)),
        "AddTranscript" => SpeechmaticsEvent::Final(transcript_of(&value)),
        "EndOfTranscript" => SpeechmaticsEvent::EndOfTranscript,
        "Error" => SpeechmaticsEvent::Error(
            value
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("speechmatics error")
                .to_owned(),
        ),
        _ => SpeechmaticsEvent::Other,
    }
}

/// Generate WAV or PCM speech audio from plain text.
pub async fn synthesize_speech(
    api_key: &str,
    tts_url: &str,
    voice: &str,
    text: &str,
    output_format: &str,
) -> Result<Vec<u8>> {
    let url = format!(
        "{}/generate/{}",
        tts_url.trim_end_matches('/'),
        voice.trim()
    );
    let response = reqwest::Client::new()
        .post(url)
        .bearer_auth(api_key)
        .query(&[("output_format", output_format)])
        .json(&json!({ "text": text }))
        .send()
        .await
        .context("speechmatics tts request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("speechmatics tts failed {status}: {body}");
    }

    let bytes = response
        .bytes()
        .await
        .context("speechmatics tts audio read failed")?;
    Ok(bytes.to_vec())
}

fn transcript_of(value: &Value) -> String {
    value
        .get("metadata")
        .and_then(|m| m.get("transcript"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}
