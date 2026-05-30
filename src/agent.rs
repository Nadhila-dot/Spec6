//! Agentic tool loop for the chat stream.
//!
//! Protocol: the LLM emits `<tool name="..." query="..." />` self-closing tags in
//! its output. The runtime parses them, executes them in parallel via BrightData,
//! and feeds the results back as a synthetic user turn containing `<tool_result>`
//! blocks. Loops until the LLM produces a pass with zero tool tags — that pass
//! is the final answer.

use crate::cognee::{CogneeClient, dataset_name_for_group};
use crate::config::AppConfig;
use crate::inference::{self, ChatRole, ChatTurn, InferenceSelection, InferenceStreamEvent, StreamOptions};
use crate::overview::{self, ChatCompanyContext, ChatToolOutcome};
use anyhow::Result;
use futures::future::BoxFuture;
use serde::Serialize;
use std::sync::Arc;

pub const MAX_AGENT_ITERATIONS: usize = 4;
pub const MAX_TOOLS_PER_TURN: usize = 4;
pub const MAX_SEED_TOOLS: usize = 6;

#[derive(Debug, Clone)]
pub struct AgentCompanyContext {
    pub group_id: Option<String>,
    pub company: Option<ChatCompanyContext>,
}

#[derive(Debug, Clone)]
pub struct ToolInvocation {
    pub id: String,
    pub name: String,
    pub source_type: String,
    pub query: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolStartedEvent {
    pub id: String,
    pub name: String,
    pub source_type: String,
    pub query: String,
    pub label: String,
    pub iteration: usize,
    pub started_at_unix_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCompletedEvent {
    pub id: String,
    pub source_type: String,
    pub query: String,
    pub result_count: usize,
    pub elapsed_ms: u128,
    pub status: String,
    pub error: Option<String>,
}

/// Map a tool `name` (as it appears in `<tool name="..."/>`) to its BrightData
/// source_type. Unknown names default to a general SERP search.
fn name_to_source_type(name: &str) -> &'static str {
    match name.trim().to_ascii_lowercase().as_str() {
        "search_reddit" | "reddit" => "reddit",
        "search_trustpilot" | "trustpilot" => "trustpilot",
        "search_g2" | "g2" => "g2",
        "search_capterra" | "capterra" => "capterra",
        "search_triggerware" | "triggerware" | "search_alerts" | "enterprise_memory" => {
            "triggerware"
        }
        "search_web" | "search" | "web" | "google" => "serp_review",
        "search_cognee" | "cognee" | "knowledge_graph" => "cognee",
        // Counterfit persona — marketplace scouts. Route through SERP with
        // site: filters until BrightData prebuilt scrapers are wired in.
        "search_amazon" | "amazon" => "amazon",
        "search_aliexpress" | "aliexpress" | "search_temu" | "temu" => "aliexpress",
        "search_ebay" | "ebay" | "search_etsy" | "etsy" => "ebay",
        "search_niche" | "niche" | "search_marketplace" => "niche",
        // Consulting persona — reputation + supplier risk + financial signals.
        "search_news" | "news" => "news",
        // Sales persona — competitive intel + social pulse.
        "search_linkedin" | "linkedin" => "linkedin",
        "search_social" | "social" | "tiktok" | "twitter" | "x" => "social",
        // Spoken Web scout — transcribe earnings calls / YouTube / podcasts.
        "search_video" | "search_media" | "media" | "video" | "youtube" | "podcast"
        | "earnings_call" => "media",
        _ => "serp_review",
    }
}

pub fn tool_label(source_type: &str, query: &str) -> String {
    let q = query.trim();
    match source_type {
        "reddit" => format!("Reddit · {q}"),
        "trustpilot" => format!("Trustpilot · {q}"),
        "g2" => format!("G2 · {q}"),
        "capterra" => format!("Capterra · {q}"),
        "triggerware" => format!("Triggerware · {q}"),
        "amazon" => format!("Amazon Scout · {q}"),
        "aliexpress" => format!("AliExpress/Temu · {q}"),
        "ebay" => format!("eBay/Etsy · {q}"),
        "niche" => format!("Niche Marketplace · {q}"),
        "news" => format!("News & Reputation · {q}"),
        "linkedin" => format!("LinkedIn · {q}"),
        "social" => format!("Social Pulse · {q}"),
        "media" => format!("Spoken Web · {q}"),
        "cognee" => format!("Knowledge Graph · {q}"),
        _ => format!("Web search · {q}"),
    }
}

/// Parse tool-call tags out of model output. Recognises the canonical form
/// `<tool name="..." query="..."/>` and several variants LLMs commonly emit
/// (e.g. `<tool_name="X" query="Y"/>`, `<tool_call …>`, `<function_call …>`,
/// `<invoke …>`, `<use_tool …>`). Any matched tag — usable or not — is stripped
/// from the visible text so junk never leaks into the rendered answer.
/// Returns (visible_text_without_tags, invocations).
pub fn parse_tool_calls(text: &str, base_id: &str) -> (String, Vec<ToolInvocation>) {
    let mut invocations = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;

    while i < text.len() {
        match find_tool_tag_span(&text[i..]) {
            Some(span) => {
                out.push_str(&text[i..i + span.start]);
                let raw_tag = &text[i + span.start..i + span.end];
                if let Some(inv) =
                    parse_tool_tag(raw_tag, &format!("{base_id}-{}", invocations.len()))
                {
                    invocations.push(inv);
                }
                // Always strip — even if we couldn't parse, this looked like a
                // tool tag and would only confuse the user if rendered.
                i += span.end;
            }
            None => {
                out.push_str(&text[i..]);
                break;
            }
        }
    }

    // Final swipe: any stray closing tags ("</tool_name>", "</tool>") still in
    // the visible text get nuked.
    let cleaned = strip_orphan_closing_tags(&out);
    (cleaned, invocations)
}

#[derive(Debug, Clone, Copy)]
struct TagSpan {
    start: usize,
    end: usize,
}

/// Locate the next tool-shaped tag in `slice`, returning its byte span.
fn find_tool_tag_span(slice: &str) -> Option<TagSpan> {
    let lower = slice.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let lt = lower[cursor..].find('<')?;
        let open = cursor + lt;
        if open + 1 >= bytes.len() {
            return None;
        }
        // Skip closing tags and comments.
        let after_lt = bytes[open + 1];
        if after_lt == b'/' || after_lt == b'!' || after_lt == b'?' {
            cursor = open + 1;
            continue;
        }
        let head_end = find_tag_head_end(&lower[open..]);
        let inner = &lower[open + 1..open + head_end];
        if is_tool_like_tag(inner) {
            // Find end of tag in original string (case-preserved).
            let original_rest = &slice[open..];
            let end_off = find_tag_close(original_rest)?;
            let end = open + end_off;
            // If it's a paired open tag (not self-closing), also strip the body
            // up to the matching close tag, if present nearby.
            let tag_name = inner
                .split(|c: char| c.is_whitespace() || c == '/' || c == '=')
                .next()
                .unwrap_or("");
            let close_marker = format!("</{tag_name}");
            let body_search = &slice[end..];
            let final_end = if !original_rest[..end_off].ends_with("/>")
                && !tag_name.is_empty()
            {
                if let Some(close_idx) = body_search.to_ascii_lowercase().find(&close_marker) {
                    // Find the '>' after that
                    if let Some(close_gt) = body_search[close_idx..].find('>') {
                        end + close_idx + close_gt + 1
                    } else {
                        end
                    }
                } else {
                    end
                }
            } else {
                end
            };
            return Some(TagSpan {
                start: open,
                end: final_end,
            });
        }
        cursor = open + 1;
    }
    None
}

/// Returns the byte offset (relative to the slice start which is `<`) at
/// which the element name + attributes end (i.e. the position just past the
/// closing `>` or `/>`).
fn find_tag_close(slice: &str) -> Option<usize> {
    // Walk forward respecting quotes so `="x>y"` doesn't trip us.
    let bytes = slice.as_bytes();
    let mut i = 1;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == b'"' || c == b'\'' {
                    quote = Some(c);
                } else if c == b'>' {
                    return Some(i + 1);
                }
            }
        }
        i += 1;
    }
    None
}

fn find_tag_head_end(slice: &str) -> usize {
    // Mirror find_tag_close but return position INCLUDING the closing >.
    find_tag_close(slice).unwrap_or(slice.len())
}

fn is_tool_like_tag(inner: &str) -> bool {
    let head: String = inner
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '=' && *c != '/' && *c != '>')
        .collect();
    let head_lower = head.to_ascii_lowercase();
    // Element name signals
    let name_signals = [
        "tool",
        "tool_call",
        "tool_use",
        "tool_name",
        "function_call",
        "function",
        "invoke",
        "use_tool",
        "search",
    ];
    if name_signals.iter().any(|p| head_lower == *p) {
        return true;
    }
    // Or: an attribute key reveals the intent. Look at the first few attr keys.
    let body = &inner[head.len()..];
    let body_lower = body.to_ascii_lowercase();
    let attr_signals = [
        "tool_name=",
        "function_name=",
        "tool=",
        "function=",
    ];
    attr_signals.iter().any(|p| body_lower.contains(p))
}

fn parse_tool_tag(tag: &str, id: &str) -> Option<ToolInvocation> {
    // Pull every attribute we recognise. The element name itself can also be
    // the tool name (e.g. `<search_reddit query="..."/>`).
    let lower = tag.to_ascii_lowercase();
    let head: String = lower[1..]
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '=' && *c != '/' && *c != '>')
        .collect();

    let name_attr = ["name", "tool_name", "function_name", "tool", "function"]
        .iter()
        .find_map(|key| extract_attr(tag, key));

    let raw_name = name_attr
        .clone()
        .unwrap_or_else(|| {
            // Element name like `search_reddit` becomes the tool name.
            if !head.is_empty()
                && head != "tool"
                && head != "tool_call"
                && head != "tool_use"
                && head != "function_call"
                && head != "function"
                && head != "invoke"
                && head != "use_tool"
                && head != "tool_name"
            {
                head.clone()
            } else {
                String::new()
            }
        });

    let query_attr = [
        "query",
        "q",
        "search_query",
        "input",
        "prompt",
        "text",
    ]
    .iter()
    .find_map(|key| extract_attr(tag, key));

    // Also handle <function_call arguments='{"query":"..."}'/> style
    let arguments_attr = extract_attr(tag, "arguments");

    let query = query_attr
        .or_else(|| {
            arguments_attr.and_then(|raw| {
                serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .and_then(|value| {
                        for key in ["query", "q", "input", "search_query", "text"] {
                            if let Some(s) = value.get(key).and_then(|v| v.as_str()) {
                                return Some(s.to_owned());
                            }
                        }
                        None
                    })
            })
        })?;

    if query.trim().is_empty() {
        return None;
    }
    let name = if raw_name.trim().is_empty() {
        "search_web".to_owned()
    } else {
        raw_name.trim().to_owned()
    };
    let source_type = name_to_source_type(&name).to_owned();
    Some(ToolInvocation {
        id: id.to_owned(),
        name,
        source_type,
        query: query.trim().to_owned(),
    })
}

fn strip_orphan_closing_tags(text: &str) -> String {
    // Remove any stray `</tool…>` `</function…>` `</invoke…>` closers.
    let mut out = String::with_capacity(text.len());
    let lower = text.to_ascii_lowercase();
    let mut i = 0;
    while i < text.len() {
        if let Some(rel) = lower[i..].find("</") {
            let abs = i + rel;
            out.push_str(&text[i..abs]);
            let after = &lower[abs + 2..];
            let head: String = after
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '>' && *c != '/')
                .collect();
            let is_tool_close = matches!(
                head.as_str(),
                "tool"
                    | "tool_call"
                    | "tool_use"
                    | "tool_name"
                    | "function_call"
                    | "function"
                    | "invoke"
                    | "use_tool"
            ) || head.starts_with("tool_")
                || head.starts_with("search_");
            if is_tool_close {
                if let Some(end) = lower[abs..].find('>') {
                    i = abs + end + 1;
                    continue;
                }
            }
            // Keep the `</` and move on.
            out.push('<');
            out.push('/');
            i = abs + 2;
        } else {
            out.push_str(&text[i..]);
            break;
        }
    }
    out
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    // Scan for `attr=` preceded by whitespace to avoid matching substrings.
    let needle = format!("{attr}=");
    let mut search = 0;
    let pos = loop {
        let idx = lower[search..].find(&needle)?;
        let abs = search + idx;
        let prev = lower[..abs].chars().last();
        if abs == 0 || matches!(prev, Some(c) if c.is_whitespace()) {
            break abs;
        }
        search = abs + needle.len();
    };
    let rest = &tag[pos + needle.len()..];
    let mut chars = rest.chars();
    let quote = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let mut value = String::new();
    for ch in chars {
        if ch == quote {
            return Some(decode_xml_entities(&value));
        }
        value.push(ch);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_self_closing_tag() {
        let text = "Let me check.\n<tool name=\"search_reddit\" query=\"puma vs nike\"/>\n";
        let (visible, calls) = parse_tool_calls(text, "id");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_reddit");
        assert_eq!(calls[0].source_type, "reddit");
        assert_eq!(calls[0].query, "puma vs nike");
        assert!(!visible.contains("<tool"));
    }

    #[test]
    fn parses_multiple_tags_in_one_pass() {
        let text = r#"<tool name="search_reddit" query="a" />
some prose
<tool name="search_trustpilot" query="b" />"#;
        let (_visible, calls) = parse_tool_calls(text, "id");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].source_type, "reddit");
        assert_eq!(calls[1].source_type, "trustpilot");
    }

    #[test]
    fn ignores_tool_result_tag() {
        let text = "<tool_result id=\"x\">stuff</tool_result>";
        let (_visible, calls) = parse_tool_calls(text, "id");
        assert!(calls.is_empty());
    }

    #[test]
    fn final_answer_with_no_tags() {
        let text = "Based on the data, Puma trails Nike on Trustpilot.";
        let (visible, calls) = parse_tool_calls(text, "id");
        assert!(calls.is_empty());
        assert_eq!(visible, text);
    }

    #[test]
    fn parses_tool_name_attribute_variant() {
        // The exact malformed form a Vultr model emitted in practice.
        let text = r#"<tool_name="search_web" query="Altare brand company" recency_days="365" /></tool_name>"#;
        let (visible, calls) = parse_tool_calls(text, "id");
        assert_eq!(calls.len(), 1, "should still extract the call");
        assert_eq!(calls[0].name, "search_web");
        assert_eq!(calls[0].source_type, "serp_review");
        assert_eq!(calls[0].query, "Altare brand company");
        assert!(
            !visible.contains("<tool") && !visible.contains("</tool"),
            "raw tag should be stripped, got: {visible:?}"
        );
    }

    #[test]
    fn parses_function_call_variant() {
        let text = r#"<function_call name="search_reddit" arguments='{"query":"Altare brand"}' />"#;
        let (visible, calls) = parse_tool_calls(text, "id");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].source_type, "reddit");
        assert_eq!(calls[0].query, "Altare brand");
        assert!(!visible.contains("<function_call"));
    }

    #[test]
    fn parses_invoke_variant() {
        let text = r#"<invoke name="search_g2" query="cohort retention tools" />"#;
        let (_visible, calls) = parse_tool_calls(text, "id");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].source_type, "g2");
    }

    #[test]
    fn paired_tool_tag_with_body() {
        let text = r#"<tool name="search_web" query="x">junk inside</tool> after"#;
        let (visible, calls) = parse_tool_calls(text, "id");
        assert_eq!(calls.len(), 1);
        assert!(visible.trim_start().starts_with("after"));
    }
}

fn decode_xml_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Build the synthetic user message body that delivers tool results back to
/// the model. Kept compact: each call gets a small JSON-ish block.
pub fn render_tool_results_turn(outcomes: &[(ToolInvocation, ChatToolOutcome)]) -> String {
    let mut lines = Vec::new();
    lines.push("<tool_results>".to_owned());
    for (inv, outcome) in outcomes {
        lines.push(format!(
            "<tool_result id=\"{}\" name=\"{}\" source=\"{}\" query=\"{}\" result_count=\"{}\" elapsed_ms=\"{}\">",
            inv.id,
            inv.name,
            outcome.source_type,
            escape_attr(&inv.query),
            outcome.result_count(),
            outcome.elapsed_ms,
        ));
        if let Some(err) = &outcome.error {
            lines.push(format!("ERROR: {err}"));
        }
        if outcome.items.is_empty() {
            lines.push("(no results)".to_owned());
        } else {
            for (idx, item) in outcome.items.iter().take(6).enumerate() {
                let url = item.url.as_deref().unwrap_or("");
                lines.push(format!(
                    "{}. {} {}",
                    idx + 1,
                    item.title.trim(),
                    if url.is_empty() {
                        String::new()
                    } else {
                        format!("({url})")
                    }
                ));
                let snippet = item.snippet.trim();
                if !snippet.is_empty() {
                    let trimmed = snippet.chars().take(360).collect::<String>();
                    lines.push(format!("   {trimmed}"));
                }
            }
        }
        lines.push("</tool_result>".to_owned());
    }
    lines.push("</tool_results>".to_owned());
    lines.push(
        "Continue. If you have enough evidence, write the final answer now (no <tool/> tags). Otherwise call more tools."
            .to_owned(),
    );
    lines.join("\n")
}

fn escape_attr(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Buffer one full pass of the model's output into a String.
pub async fn collect_pass(
    config: &AppConfig,
    selection: &InferenceSelection,
    turns: &[ChatTurn],
    group_data_text: Option<&str>,
) -> Result<String> {
    let mut buffer = String::new();
    inference::stream_text_ex(
        config,
        selection,
        turns,
        group_data_text,
        StreamOptions {
            // Title is generated after the loop via a fallback heuristic — turn
            // off the streamed-meta protocol so the model doesn't emit <meta>
            // inside agent passes.
            include_streamed_meta: Some(false),
            include_tools: true,
        },
        |event| -> BoxFuture<'static, Result<()>> {
            if let InferenceStreamEvent::TextDelta(delta) = event {
                buffer.push_str(&delta);
            }
            Box::pin(async move { Ok(()) })
        },
    )
    .await?;
    Ok(buffer)
}

pub fn now_unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Run all tool invocations for one turn in parallel, returning outcomes in
/// the same order as `invocations`.
pub async fn run_tools(
    config: &AppConfig,
    invocations: &[ToolInvocation],
    cognee: Option<Arc<CogneeClient>>,
    context: Option<&AgentCompanyContext>,
) -> Vec<ChatToolOutcome> {
    use futures::future::join_all;
    let futures = invocations.iter().map(|inv| {
        let cognee_clone = cognee.clone();
        let group_id_owned = context.and_then(|ctx| ctx.group_id.clone());
        let company_context = context.and_then(|ctx| ctx.company.clone());
        async move {
            if inv.source_type == "cognee" {
                run_cognee_tool(cognee_clone.as_deref(), group_id_owned.as_deref(), &inv.query).await
            } else {
                overview::run_chat_tool(
                    config,
                    &inv.source_type,
                    &inv.query,
                    company_context.as_ref(),
                )
                .await
            }
        }
    });
    join_all(futures).await
}

async fn run_cognee_tool(
    cognee: Option<&CogneeClient>,
    group_id: Option<&str>,
    query: &str,
) -> ChatToolOutcome {
    let start = std::time::Instant::now();
    let Some(client) = cognee else {
        return ChatToolOutcome {
            source_type: "cognee".to_owned(),
            query: query.to_owned(),
            items: vec![],
            elapsed_ms: 0,
            error: Some("Cognee not configured".to_owned()),
        };
    };
    let Some(gid) = group_id else {
        return ChatToolOutcome {
            source_type: "cognee".to_owned(),
            query: query.to_owned(),
            items: vec![],
            elapsed_ms: 0,
            error: Some("No company context for Cognee search".to_owned()),
        };
    };
    let dataset = dataset_name_for_group(gid);
    match client.search(query, &dataset, "GRAPH_COMPLETION").await {
        Ok(results) => ChatToolOutcome {
            source_type: "cognee".to_owned(),
            query: query.to_owned(),
            items: results
                .into_iter()
                .map(|r| overview::ToolResultItem {
                    title: "Knowledge Graph".to_owned(),
                    url: None,
                    snippet: r.text,
                })
                .collect(),
            elapsed_ms: start.elapsed().as_millis(),
            error: None,
        },
        Err(e) => ChatToolOutcome {
            source_type: "cognee".to_owned(),
            query: query.to_owned(),
            items: vec![],
            elapsed_ms: start.elapsed().as_millis(),
            error: Some(e.to_string()),
        },
    }
}

/// Helper for the `Assistant` turn appended to history between iterations —
/// preserves what the model said (including its tool tags) so subsequent
/// passes see their own prior reasoning.
pub fn assistant_turn_from_pass(pass: &str) -> ChatTurn {
    ChatTurn {
        role: ChatRole::Assistant,
        body: pass.to_owned(),
    }
}

pub fn user_tool_results_turn(body: String) -> ChatTurn {
    ChatTurn {
        role: ChatRole::User,
        body,
    }
}

/// Deterministic agent routing.
///
/// Open-source models tend to fall back to `search_web` even when the prompt
/// lists specialised tools. For high-signal intents (counterfeit, competitive,
/// supply-chain, reputation crisis) we seed the first iteration with the
/// correct fan-out so the UI lights up the right persona row and the LLM gets
/// the right evidence regardless of its tool-routing taste.
///
/// `brand_context` is the chat-group name when one exists — used to flavor the
/// queries when the user message is short ("any fakes?") and lacks a subject.
pub fn seed_invocations_for_user_message(
    user_message: &str,
    company_context: Option<&ChatCompanyContext>,
    base_id: &str,
) -> Vec<ToolInvocation> {
    let text = user_message.to_lowercase();
    let subject = pick_subject(user_message, company_context);
    let context_hints = company_context
        .map(overview::chat_company_query_hints)
        .unwrap_or_default();

    let intent = classify_intent(&text);
    if matches!(intent, Intent::None) {
        return Vec::new();
    }

    let agents: Vec<(&str, &str, &str)> = match intent {
        Intent::Trigger => vec![
            ("search_triggerware", "triggerware", "recent changes alerts deltas monitors"),
            ("search_cognee", "cognee", "saved alerts history competitor memory risks"),
            ("search_news", "news", "recent breaking changes launch lawsuit backlash"),
            ("search_web", "serp_review", "pricing review complaint change latest"),
        ],
        Intent::Counterfeit => vec![
            ("search_amazon", "amazon", "counterfeit listings fake replica"),
            ("search_aliexpress", "aliexpress", "fake supplier clusters replica"),
            ("search_ebay", "ebay", "resale fakes counterfeit"),
            ("search_niche", "niche", "regional marketplace fakes"),
        ],
        Intent::Competitive => vec![
            ("search_news", "news", "competitor pricing change product launch recent"),
            ("search_linkedin", "linkedin", "competitor executive hires VP director joined"),
            ("search_web", "serp_review", "market share vs competitors latest"),
            ("search_social", "social", "viral mentions competitor comparison"),
        ],
        Intent::Reputation => vec![
            ("search_reddit", "reddit", "customer complaints product issues thread"),
            ("search_trustpilot", "trustpilot", "ratings reviews complaints latest"),
            ("search_news", "news", "negative press crisis backlash recent"),
        ],
        Intent::Supply => vec![
            ("search_news", "news", "supplier sanctions OFAC factory incident"),
            ("search_web", "serp_review", "supply chain disruption sourcing risk"),
            ("search_web", "serp_review", "supplier financial distress bankruptcy"),
        ],
        Intent::Geographic => vec![
            // User explicitly wants the map populated. Hammer revenue-by-region
            // queries and known-market signals.
            ("search_web", "serp_review", "revenue by region geographic breakdown EMEA Americas APAC"),
            ("search_web", "serp_review", "sales by country top markets share"),
            ("search_news", "news", "regional performance growth Europe China US India"),
            ("search_web", "serp_review", "export markets distribution countries"),
            ("search_social", "social", "regional buzz top markets demand"),
            ("search_linkedin", "linkedin", "regional executives country-manager"),
        ],
        Intent::Sales => vec![
            // Hammer specific financial-data queries. Generic "demand signals"
            // never returns revenue figures — these are SEC-filing flavoured.
            ("search_web", "serp_review", "annual revenue full year results latest"),
            ("search_news", "news", "quarterly earnings sales growth Q3 Q4 2025"),
            ("search_web", "serp_review", "market share regional breakdown EMEA Americas APAC"),
            ("search_web", "serp_review", "DTC direct-to-consumer wholesale split percentage"),
            ("search_linkedin", "linkedin", "executive commentary outlook guidance"),
            ("search_social", "social", "regional demand buzz top markets"),
        ],
        Intent::None => Vec::new(),
    };

    agents
        .into_iter()
        .take(MAX_SEED_TOOLS)
        .enumerate()
        .map(|(idx, (name, source_type, hint))| {
            let query = format_query(&subject, hint, &context_hints);
            ToolInvocation {
                id: format!("{base_id}-seed-{idx}"),
                name: name.to_owned(),
                source_type: source_type.to_owned(),
                query,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum Intent {
    None,
    Trigger,
    Counterfeit,
    Competitive,
    Reputation,
    Supply,
    Sales,
    Geographic,
}

fn classify_intent(lower: &str) -> Intent {
    // Map-explicit asks beat every other classifier. If the user is staring at
    // the map and yelling "plot it", we route to a focused geographic fan-out
    // that produces revenue-by-region data.
    if is_map_request(lower) {
        return Intent::Geographic;
    }

    let trigger_terms = [
        "trigger",
        "triggers",
        "alert",
        "alerts",
        "monitor",
        "monitoring",
        "what changed",
        "delta",
        "notify",
        "notification",
        "slack",
        "discord",
        "webhook",
        "watch this",
        "keep an eye",
    ];
    if trigger_terms.iter().any(|t| lower.contains(t)) {
        return Intent::Trigger;
    }

    let counterfeit_terms = [
        "counterfeit",
        "counterfit",
        "fake",
        "fakes",
        "knockoff",
        "knock-off",
        "knock off",
        "replica",
        "replicas",
        "dupe ",
        "dupes",
        "brand protection",
        "ip infringement",
        "trademark",
        "dmca",
    ];
    if counterfeit_terms.iter().any(|t| lower.contains(t)) {
        return Intent::Counterfeit;
    }

    let competitive_terms = [
        "competitor",
        "competitive",
        "rival",
        "market share",
        "pricing change",
        "hiring",
        "launched",
        "vs ",
    ];
    if competitive_terms.iter().any(|t| lower.contains(t)) {
        return Intent::Competitive;
    }

    let supply_terms = [
        "supplier",
        "supply chain",
        "sanctions",
        "ofac",
        "factory",
        "vendor risk",
        "tier-1",
        "tier 1",
    ];
    if supply_terms.iter().any(|t| lower.contains(t)) {
        return Intent::Supply;
    }

    let reputation_terms = [
        "reputation",
        "sentiment",
        "complaints",
        "trustpilot",
        "review",
        "reviews",
        "crisis",
        "scandal",
        "backlash",
        "boycott",
    ];
    if reputation_terms.iter().any(|t| lower.contains(t)) {
        return Intent::Reputation;
    }

    let sales_terms = [
        "sales",
        "demand",
        "popular",
        "trending",
        "where do we sell",
        "hot spot",
        "heat spot",
        "heatspot",
        "heat-spot",
        "market hot",
    ];
    if sales_terms.iter().any(|t| lower.contains(t)) {
        return Intent::Sales;
    }

    Intent::None
}

fn pick_subject(user_message: &str, company_context: Option<&ChatCompanyContext>) -> String {
    let trimmed = user_message.trim();
    if let Some(context) = company_context {
        let search_subject = overview::chat_company_search_subject(context);
        if trimmed.split_whitespace().count() <= 6 {
            return search_subject;
        }
        let brand = context.company_name.trim();
        if !brand.is_empty() {
            return search_subject;
        }
    }
    // If the message is short, use the brand context (group name).
    if let Some(brand) = company_context
        .map(|ctx| ctx.company_name.as_str())
        .map(str::trim)
        .filter(|b| !b.is_empty())
    {
        if trimmed.split_whitespace().count() <= 3 {
            return brand.to_owned();
        }
        // Otherwise prefer the brand if we have one, since this is a brand
        // intelligence product.
        return brand.to_owned();
    }
    // Fallback: try to extract a capitalised proper noun from the message.
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let proper: Vec<&str> = tokens
        .iter()
        .filter(|t| {
            let first = t.chars().next();
            first.map(|c| c.is_uppercase()).unwrap_or(false)
                && t.chars().filter(|c| c.is_alphanumeric()).count() >= 2
        })
        .copied()
        .collect();
    if !proper.is_empty() {
        return proper.join(" ");
    }
    // Last resort: trim the message into a short query.
    trimmed.chars().take(60).collect()
}

/// True when the user explicitly wants the answer plotted on the map.
pub fn is_map_request(lower: &str) -> bool {
    let triggers = [
        "plot",
        "on a map",
        "on the map",
        "on map",
        "map it",
        "map this",
        "map that",
        "show on map",
        "show on a map",
        "show me a map",
        "draw the map",
        "use the map",
        "geograph",
        "where do",
        "where are",
        "where is",
        "which countr",
        "which region",
        "which market",
        "by region",
        "by country",
        "by market",
        "regional breakdown",
        "country breakdown",
        "market breakdown",
        "global breakdown",
    ];
    triggers.iter().any(|t| lower.contains(t))
}

fn format_query(subject: &str, hint: &str, context_hints: &[String]) -> String {
    let subj = subject.trim();
    if subj.is_empty() {
        return hint.to_owned();
    }
    if let Some(anchor) = context_hints.first() {
        format!("{subj} {hint} {anchor}")
    } else {
        format!("{subj} {hint}")
    }
}

#[cfg(test)]
mod seed_tests {
    use super::*;

    #[test]
    fn counterfeit_keyword_seeds_four_marketplace_scouts() {
        let inv =
            seed_invocations_for_user_message("whats the status on counterfits and fakes?", Some("Puma"), "x");
        assert_eq!(inv.len(), 4);
        let names: Vec<&str> = inv.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"search_amazon"));
        assert!(names.contains(&"search_aliexpress"));
        assert!(names.contains(&"search_ebay"));
        assert!(names.contains(&"search_niche"));
        assert!(inv.iter().all(|i| i.query.contains("Puma")));
    }

    #[test]
    fn brand_question_with_no_intent_seeds_nothing() {
        let inv = seed_invocations_for_user_message("hello", Some("Puma"), "x");
        assert!(inv.is_empty());
    }

    #[test]
    fn competitive_keyword_routes_to_sales_persona() {
        let inv =
            seed_invocations_for_user_message("what are competitors doing this quarter?", Some("Nike"), "x");
        let names: Vec<&str> = inv.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"search_linkedin"));
        assert!(names.contains(&"search_social"));
    }
}
