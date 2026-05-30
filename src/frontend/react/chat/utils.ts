import type { ChatMessage, InferenceCatalog } from "../types";

/* ─── debug logging ──────────────────────────────────────────────────────── */

declare global {
  interface Window {
    __WW_DEBUG_STREAM?: boolean;
    __WW_STREAM_LOGS?: Array<{ at: string; label: string; data?: unknown }>;
    __WW_DUMP_STREAM?: () => string;
  }
}

export function debugStreamEnabled(): boolean {
  if (typeof window === "undefined") return false;
  return (
    window.__WW_DEBUG_STREAM === true ||
    window.localStorage.getItem("ww:debug-stream") === "1"
  );
}

export function debugStream(label: string, data?: unknown) {
  if (!debugStreamEnabled()) return;
  const entry = { at: new Date().toISOString(), label, data };
  window.__WW_STREAM_LOGS ??= [];
  window.__WW_STREAM_LOGS.push(entry);
  if (window.__WW_STREAM_LOGS.length > 500)
    window.__WW_STREAM_LOGS.splice(0, window.__WW_STREAM_LOGS.length - 500);
  window.__WW_DUMP_STREAM = () =>
    JSON.stringify(window.__WW_STREAM_LOGS ?? [], null, 2);
  if (data === undefined) console.log(`[ww:stream] ${label}`);
  else console.log(`[ww:stream] ${label}`, data);
}

export function logStreamTiming(label: string, data?: Record<string, unknown>) {
  if (typeof window === "undefined") return;
  console.log(`[ww:stream-timing] ${label}`, {
    client_unix_ms: Date.now(),
    client_performance_ms: performance.now(),
    ...data,
  });
}

/* ─── greeting ───────────────────────────────────────────────────────────── */

export function buildGreeting(name: string): string {
  const first = name.trim().split(/\s+/)[0] || "there";
  const hour = new Date().getHours();
  const slot =
    hour < 12
      ? "Just woke up?"
      : hour < 17
        ? "Afternoon chat about something?"
        : "Good evening";
  return `${slot}, ${first}`;
}

export function firstName(displayName: string, username: string): string {
  const name = (displayName || username || "").trim();
  return name.split(/\s+/)[0] || "there";
}

/* ─── narration scrubber ─────────────────────────────────────────────────── */

/**
 * The LLM sometimes leaks raw <tool /> tags or narrates tool calls as prose
 * ("🔍 Searching reddit for…"). The agentic runtime catches the legitimate
 * <tool/> form before it streams, but stray fragments slip through. This scrub
 * is the last line of defense before display.
 */
export function stripFakeToolNarration(body: string): string {
  let cleaned = body;

  // 1. Strip ANY well-formed <sentinel-*>…</sentinel-*> block. Pins and trend
  //    are rendered separately; any other variant (incl. hallucinated tags like
  //    <sentinel-pend>) is scaffolding that must never reach the reader.
  cleaned = cleaned.replace(
    /<sentinel-[a-z-]+>[\s\S]*?<\/sentinel-[a-z-]+>/gi,
    "",
  );
  // 2. Strip an unclosed sentinel block (model ran out of tokens before the
  //    closing tag). Anything from the opening tag to EOF is junk.
  cleaned = cleaned.replace(/<sentinel-[a-z-]+>[\s\S]*$/i, "");
  // 3. Strip any stray opener/closer left behind.
  cleaned = cleaned.replace(/<\/?sentinel-[a-z-]+>/gi, "");

  // 3. Strip echoed tool-result envelopes — some models parrot the prior
  //    <tool_results>…</tool_results> block back into their answer.
  cleaned = cleaned.replace(/<tool_results>[\s\S]*?<\/tool_results>/gi, "");
  cleaned = cleaned.replace(/<tool_result\b[\s\S]*?<\/tool_result>/gi, "");
  cleaned = cleaned.replace(/<\/?tool_results?>/gi, "");

  // 4. Strip stray <tool .../> remnants.
  cleaned = cleaned.replace(/<tool\s+[^>]*\/?>/gi, "");
  cleaned = cleaned.replace(/<\/tool>/gi, "");

  // 5. Strip <think>/<thinking> blocks (handled separately by splitThinking
  //    when present at the top of the message — but if they appear mid-body
  //    as leftover scaffolding, drop them here too).
  cleaned = cleaned.replace(/<think(?:ing)?>[\s\S]*?<\/think(?:ing)?>/gi, "");
  // Unclosed think block: drop from tag to EOF.
  cleaned = cleaned.replace(/<think(?:ing)?>[\s\S]*$/i, "");
  cleaned = cleaned.replace(/<\/think(?:ing)?>/gi, "");

  // 6. Strip orphan pin-section headings: the model often writes
  //    "## Brand — Geographic Revenue Pins" at the very end intending to drop
  //    a pin block under it, then forgets to. The heading without content
  //    looks like a UI bug. Match a trailing ##/###/h1 line whose text contains
  //    "pin"/"pins" near "geograph"/"map"/"location"/"region"/"country", with
  //    no further content beneath, and drop it.
  cleaned = cleaned.replace(
    /\n#{1,6}\s+[^\n]*?\b(?:geograph\w*|map|location|region\w*|country|countries|world)\b[^\n]*?\bpins?\b[^\n]*\s*$/i,
    "",
  );
  cleaned = cleaned.replace(
    /\n#{1,6}\s+[^\n]*?\bpins?\b[^\n]*?\b(?:geograph\w*|map|location|region\w*|country|countries|world)\b[^\n]*\s*$/i,
    "",
  );

  return cleaned
    .split("\n")
    .filter((line) => {
      const t = line.trim();
      if (!t) return true;
      if (/^[\u{1F50D}\u{1F50E}\u{1F310}\u{1F4CA}]\s/u.test(t)) return false;
      if (/^(Searching|Looking up|Calling|Fetching|Checking)\s/i.test(t))
        return false;
      return true;
    })
    .join("\n");
}

/* ─── AI-emitted map pins ────────────────────────────────────────────────── */

export type MapPinTone = "critical" | "high" | "medium" | "low";

export interface ParsedMapPin {
  iso?: string;
  lat?: number;
  lng?: number;
  tone: MapPinTone;
  label?: string;
  md: string;
}

/**
 * Extract pins from a `<sentinel-pins>` block. Robust to:
 *  • the standard closed form `<sentinel-pins>[…]</sentinel-pins>`
 *  • a truncated open form `<sentinel-pins>[…` (model ran out of tokens) —
 *    we still salvage whatever complete pin objects were emitted before
 *    the cutoff.
 * Never throws.
 */
export function extractMapPins(body: string): ParsedMapPin[] {
  const openIdx = body.search(/<sentinel-pins>/i);
  if (openIdx < 0) return [];
  const afterOpen = body.slice(openIdx + "<sentinel-pins>".length);
  const closeMatch = afterOpen.match(/<\/sentinel-pins>/i);
  const raw = (closeMatch ? afterOpen.slice(0, closeMatch.index) : afterOpen).trim();
  if (!raw) return [];

  const repaired = repairPinJson(raw);
  let parsed: unknown;
  try {
    parsed = JSON.parse(repaired);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];

  const out: ParsedMapPin[] = [];
  for (const item of parsed) {
    if (!item || typeof item !== "object") continue;
    const rec = item as Record<string, unknown>;
    const md = typeof rec.md === "string" ? rec.md.trim() : "";
    if (!md) continue;
    const tone = (rec.tone as MapPinTone) ?? "medium";
    if (!["critical", "high", "medium", "low"].includes(tone)) continue;
    const iso = typeof rec.iso === "string" ? rec.iso.padStart(3, "0") : undefined;
    const lat = typeof rec.lat === "number" ? rec.lat : undefined;
    const lng = typeof rec.lng === "number" ? rec.lng : undefined;
    if (!iso && (lat === undefined || lng === undefined)) continue;
    out.push({
      iso,
      lat,
      lng,
      tone,
      label: typeof rec.label === "string" ? rec.label : undefined,
      md,
    });
  }
  return out;
}

/**
 * Pull the raw inner payload of a `<sentinel-trend>` block (the growth-chart
 * JSON). Tolerates a missing close tag (model ran out of tokens). Returns null
 * when no block is present. Parsing into a chart happens in trend-chart.tsx.
 */
export function extractTrendRaw(body: string): string | null {
  const openIdx = body.search(/<sentinel-trend>/i);
  if (openIdx < 0) return null;
  const afterOpen = body.slice(openIdx + "<sentinel-trend>".length);
  const closeMatch = afterOpen.match(/<\/sentinel-trend>/i);
  const raw = (closeMatch ? afterOpen.slice(0, closeMatch.index) : afterOpen).trim();
  return raw || null;
}

/**
 * Pull EVERY `<sentinel-trend>` block out of a message (the model may emit up
 * to 3). Salvages a trailing unclosed block when the stream was cut off.
 */
export function extractTrends(body: string): string[] {
  const out: string[] = [];
  const re = /<sentinel-trend>([\s\S]*?)<\/sentinel-trend>/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(body)) !== null) {
    const raw = m[1].trim();
    if (raw) out.push(raw);
  }
  if (out.length === 0) {
    const raw = extractTrendRaw(body);
    if (raw) out.push(raw);
  }
  return out;
}

/**
 * Best-effort repair of a possibly-truncated JSON array of pin objects.
 * Strategy: walk the string respecting strings & escapes, find the last
 * position where bracket/brace counts are valid, and close the array there.
 */
function repairPinJson(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed.startsWith("[")) return trimmed;
  try {
    JSON.parse(trimmed);
    return trimmed;
  } catch {
    // fall through to repair
  }

  let inString = false;
  let escape = false;
  let depthArr = 0;
  let depthObj = 0;
  let lastSafe = -1;

  for (let i = 0; i < trimmed.length; i++) {
    const ch = trimmed[i];
    if (escape) {
      escape = false;
      continue;
    }
    if (ch === "\\" && inString) {
      escape = true;
      continue;
    }
    if (ch === '"') {
      inString = !inString;
      continue;
    }
    if (inString) continue;
    if (ch === "[") depthArr++;
    else if (ch === "]") depthArr--;
    else if (ch === "{") depthObj++;
    else if (ch === "}") {
      depthObj--;
      // Just closed an object at the top level inside the array — safe stop.
      if (depthObj === 0 && depthArr === 1) lastSafe = i;
    }
  }

  if (lastSafe < 0) return trimmed; // nothing to salvage
  return trimmed.slice(0, lastSafe + 1) + "]";
}

/* ─── <thinking> block extraction ────────────────────────────────────────── */

export function splitThinking(body: string): { thinking: string; answer: string } {
  // Accept both <think> (Qwen / DeepSeek / Vultr open models) and <thinking>
  // (Anthropic-flavoured) as the same construct.
  const leadingMatch = body.match(/^\s*<think(?:ing)?>([\s\S]*?)<\/think(?:ing)?>\s*/i);
  if (leadingMatch) {
    return {
      thinking: leadingMatch[1].trim(),
      answer: body.slice(leadingMatch[0].length),
    };
  }
  // Any thinking blocks anywhere — concat all, strip from answer.
  const blocks: string[] = [];
  const cleaned = body.replace(
    /<think(?:ing)?>([\s\S]*?)<\/think(?:ing)?>/gi,
    (_m, inner) => {
      blocks.push(String(inner).trim());
      return "";
    },
  );
  return {
    thinking: blocks.join("\n\n").trim(),
    answer: cleaned.trim(),
  };
}

/* ─── inference selection ────────────────────────────────────────────────── */

export function selectInferenceChoice(
  catalog: InferenceCatalog,
  providerId: string | null,
  modelId: string,
): { providerId: string | null; modelId: string } {
  const available = catalog.providers.filter((p) => p.available);
  if (available.length === 0) return { providerId: null, modelId: "" };
  const provider =
    available.find((p) => p.id === providerId) ??
    available.find((p) => p.id === catalog.default_provider) ??
    available[0];
  const nextModel =
    provider.models.find((m) => m.id === modelId)?.id ??
    provider.default_model ??
    provider.models[0]?.id ??
    "";
  return { providerId: provider.id, modelId: nextModel };
}

/* ─── quick title from first user message ───────────────────────────────── */

/**
 * The backend's agent loop disables the streamed `<meta>` protocol because
 * it conflicts with `<tool/>` syntax — so the real title only arrives at the
 * very end of the stream (via `meta` then `done`). To avoid the sidebar
 * sitting on "New chat" for the duration of a multi-second tool run, we
 * compute a quick heuristic title client-side from the first user message
 * and apply it optimistically. The server's title (if any) overrides it
 * when the `meta` event arrives.
 *
 * Mirrors `prompt::fallback_title_from_topic` in src/prompt.rs.
 */
const NOISE_WORDS = new Set([
  "a","an","the","and","or","but","to","for","of","in","on","with","about","into",
  "from","my","our","your","me","i","we","you","it","is","are","was","were","be",
  "being","been","this","that","these","those","can","could","would","should","do",
  "does","did","help","please","need","want","make","give","tell","show","write",
  "create","draft","sketch",
]);

const ISSUE_SIGNALS = new Set([
  "issue","issues","problem","problems","broken","fails","failure","stuck","late",
  "error","errors","bug","bugs","bad","sucks","wrong",
]);

export function quickTitleFromMessage(message: string): string | null {
  const tokens = message
    .split(/[^\p{L}\p{N}]+/u)
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
  if (tokens.length === 0) return null;

  const hasIssue = tokens.some((t) => ISSUE_SIGNALS.has(t.toLowerCase()));

  const subject = tokens
    .filter((t) => {
      const k = t.toLowerCase();
      return !NOISE_WORDS.has(k) && !ISSUE_SIGNALS.has(k);
    })
    .slice(0, 4);

  if (subject.length === 0) return null;

  const parts = subject.map((t) => {
    if (/^[A-Z0-9]+$/.test(t)) return t;
    return t[0].toUpperCase() + t.slice(1).toLowerCase();
  });
  if (hasIssue) parts.push("Issues");

  const title = parts.join(" ").trim();
  if (!title || title.toLowerCase() === "new chat") return null;
  return title.slice(0, 80);
}

/* ─── messages helpers ───────────────────────────────────────────────────── */

export function dedupeMessages(messages: ChatMessage[]): ChatMessage[] {
  const seen = new Set<string>();
  const out: ChatMessage[] = [];
  for (const m of messages) {
    if (seen.has(m.id)) continue;
    seen.add(m.id);
    out.push(m);
  }
  return out;
}

/* ─── tool-call presentation helpers ─────────────────────────────────────── */

export function formatElapsed(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export function sourceTypeLabel(toolName: string): string {
  const map: Record<string, string> = {
    search_reddit: "Reddit",
    search_trustpilot: "Trustpilot",
    search_g2: "G2",
    search_capterra: "Capterra",
    search_web: "Web",
    search_cognee: "Knowledge Graph",
    cognee: "Knowledge Graph",
    knowledge_graph: "Knowledge Graph",
    search_amazon: "Amazon Scout",
    search_aliexpress: "AliExpress / Temu",
    search_ebay: "eBay / Etsy",
    search_niche: "Niche Marketplace",
    search_news: "News & Reputation",
    search_linkedin: "LinkedIn",
    search_social: "Social Pulse",
    search_video: "Spoken Web",
    search_media: "Spoken Web",
    media: "Spoken Web",
  };
  return map[toolName] ?? toolName;
}

export function isCogneeTool(toolName: string): boolean {
  return toolName === "search_cognee" || toolName === "cognee" || toolName === "knowledge_graph";
}

/**
 * Map a tool's source_type (the canonical key produced by the Rust agent loop)
 * to the agent slot in the eight-agent persona roster. Returns null for the
 * Cognee/web fallbacks which render as the central orchestrator rather than
 * a persona card.
 *
 * Source-of-truth tool names live in src/agent.rs `name_to_source_type`.
 */
export function sourceTypeToAgentId(sourceType: string | undefined): string | null {
  switch ((sourceType ?? "").toLowerCase()) {
    case "amazon":
      return "amazon";
    case "aliexpress":
      return "aliexpress";
    case "ebay":
      return "ebay";
    case "niche":
      return "niche";
    case "news":
    case "serp_review":
    case "trustpilot":
      return "news";
    case "supply":
      return "supply";
    case "linkedin":
    case "g2":
    case "capterra":
      return "competitor";
    case "reddit":
    case "social":
      return "social";
    case "media":
      return "media";
    default:
      return null;
  }
}
