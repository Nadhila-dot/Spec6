/**
 * Wire-level event contract for the Sentinel scan stream.
 *
 * Same shape we'll use over SSE once the backend ships. Build the UI against
 * this typed union and the mock emitter; swap the source at the seam.
 *
 * Source of truth: product.md "SSE Event Contract" section.
 */

export type ThreatCategory =
  | "counterfeit"
  | "competitive"
  | "reputation"
  | "supply";

export type ThreatSeverity = "critical" | "high" | "medium" | "low";

export type AgentStatus = "queued" | "running" | "done" | "failed";

export type AgentPersona = "counterfit" | "consulting" | "sales";

export interface AgentDescriptor {
  id: string;
  label: string;
  /** BrightData product behind this agent — surface for the demo. */
  tool: string;
  persona: AgentPersona;
  category: ThreatCategory;
}

export interface ThreatItem {
  id: string;
  category: ThreatCategory;
  severity: ThreatSeverity;
  headline: string;
  source: string;
  source_url: string;
  timestamp: number;
  geo?: { lat: number; lng: number; label?: string };
  evidence_urls?: string[];
  ai_context?: string;
  recommended_actions?: string[];
}

/* ─── event union ───────────────────────────────────────────────────────── */

export interface AgentStatusEvent {
  type: "agent_status";
  agent_id: string;
  status: AgentStatus;
  message: string;
  items_found: number;
  elapsed_ms: number;
}

export interface ThreatFoundEvent {
  type: "threat_found";
  agent_id: string;
  threat: ThreatItem;
}

export interface LogEvent {
  type: "log";
  timestamp: number;
  agent_id: string;
  message: string;
}

export interface CompleteEvent {
  type: "complete";
  scan_id: string;
  overall_severity: ThreatSeverity;
  total_threats: number;
  critical: number;
  high: number;
  medium: number;
  low: number;
  dossier_markdown: string;
  elapsed_ms: number;
}

export type ScanEvent =
  | AgentStatusEvent
  | ThreatFoundEvent
  | LogEvent
  | CompleteEvent;

/* ─── the eight agents, grouped by persona ──────────────────────────────── */

export const AGENT_ROSTER: AgentDescriptor[] = [
  /* Counterfit persona — four marketplace scouts */
  {
    id: "amazon",
    label: "Amazon Scout",
    tool: "Web Scraper API",
    persona: "counterfit",
    category: "counterfeit",
  },
  {
    id: "aliexpress",
    label: "AliExpress / Temu",
    tool: "Web Unlocker + Proxies",
    persona: "counterfit",
    category: "counterfeit",
  },
  {
    id: "ebay",
    label: "eBay / Etsy",
    tool: "Web Scraper API",
    persona: "counterfit",
    category: "counterfeit",
  },
  {
    id: "niche",
    label: "Niche Marketplace",
    tool: "Scraper Studio",
    persona: "counterfit",
    category: "counterfeit",
  },

  /* Consulting persona — reputation + supplier risk */
  {
    id: "news",
    label: "News & Reputation",
    tool: "SERP API",
    persona: "consulting",
    category: "reputation",
  },
  {
    id: "supply",
    label: "Supply Chain Risk",
    tool: "SERP API + Web Unlocker",
    persona: "consulting",
    category: "supply",
  },

  /* Sales persona — heat-spots on the market */
  {
    id: "competitor",
    label: "Competitive Intelligence",
    tool: "Web Scraper API (LinkedIn)",
    persona: "sales",
    category: "competitive",
  },
  {
    id: "social",
    label: "Social Pulse",
    tool: "Scraping Browser",
    persona: "sales",
    category: "competitive",
  },
  {
    id: "media",
    label: "Spoken Web",
    tool: "Speechmatics + Scraping Browser",
    persona: "sales",
    category: "competitive",
  },
];

export const PERSONA_META: Record<
  AgentPersona,
  { label: string; tagline: string; accent: string }
> = {
  counterfit: {
    label: "Counterfit",
    tagline: "Fakes, similar listings, and seller signals.",
    accent: "rose",
  },
  consulting: {
    label: "Consulting",
    tagline: "Reputation, supplier risk, financial signals.",
    accent: "amber",
  },
  sales: {
    label: "Sales",
    tagline: "Competitive heat-spots and social pulse.",
    accent: "emerald",
  },
};

export const SEVERITY_META: Record<
  ThreatSeverity,
  { label: string; tone: string }
> = {
  critical: { label: "Critical", tone: "text-rose-400 bg-rose-500/10" },
  high: { label: "High", tone: "text-orange-400 bg-orange-500/10" },
  medium: { label: "Medium", tone: "text-amber-400 bg-amber-500/10" },
  low: { label: "Low", tone: "text-sky-400 bg-sky-500/10" },
};
