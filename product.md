# Spec6 — Product Spec
### Web Data UNLOCKED Hackathon · May 25–31, 2026
### lablab.ai × BrightData

---

## TL;DR

Spec6 is an **autonomous brand-intelligence analyst you talk to**. Ask it anything about a brand — "any counterfeits on Puma?", "how are our global sales trending?", "what are competitors shipping this quarter?" — and it fans specialised agents across BrightData's web infrastructure, reasons over the evidence, and answers like a McKinsey/Bloomberg analyst: a written verdict, a live **world threat map**, **growth-projection charts**, and a citable evidence trail. Then it keeps working *after* you close the tab — a background **Watchtower** re-scans every tracked company on a schedule, with zero human in the loop.

**Tagline:** *"The Bloomberg Terminal for brand threats — that you can just talk to."*

> **Note on the chat pivot:** v1.0 of this spec described a one-shot "type a brand → get a dossier" flow. We shipped something stronger: a **conversational** surface where the dossier is assembled live, in-thread, across a real agentic tool loop. The map, charts, knowledge graph, and autonomous monitoring all hang off the conversation. Everything below reflects what's actually built.

---

## Prize Structure

- **Grand Prize: $5,000 cash** — single best project across all tracks.
- **AI Startup Program: up to $20,000 in BrightData credits.**
- Every participant gets **$250 in BrightData API credits** on Day 1.
- One winner overall. Strategy is "best submission in the room," not "win one track."

---

## Stack (as built)

- **Backend:** Rust (Axum + Tokio). Orchestrator, agentic tool loop, BrightData overview pipeline, autonomous Watchtower, SSE + WebSocket streaming.
- **Frontend shell:** Rust SSR — serves HTML, injects initial state via `window.dataSSr`, React hydrates.
- **Frontend UI:** React 19 + TypeScript + Vite. Streaming chat, world heatmap (`react-simple-maps`), **growth charts (`recharts`)**, agent canvas drawer.
- **Knowledge graph:** Cognee — every conversation + completed overview is ingested into a per-company graph the agent can query back (`search_cognee`).
- **Persistence:** MongoDB (users, conversations, messages, companies, overviews).
- **Inference:** pluggable — Gemini or Vultr, selectable per message from the composer.
- **Web data:** BrightData — full suite (below).

---

## How it actually works

1. **Onboard a company.** Create a "company group" with a name + free-text profile (specialty, customers, known competitors). On save, Spec6 auto-queues a **BrightData competitor overview** in the background and ingests the profile into the company's Cognee graph.
2. **Ask anything in chat.** Each message runs an **agentic tool loop** in Rust:
   - A deterministic **router** pre-dispatches the right specialised scouts based on intent (counterfeit → Amazon/AliExpress/eBay/Niche; competitive → LinkedIn/News/Social; reputation → Reddit/Trustpilot/News; sales/geo → revenue-by-region SERP).
   - The LLM then runs up to 4 iterations × 4 tools, calling more scouts or `search_cognee` until it has enough evidence.
   - Tool calls stream to the UI live as a **per-agent canvas** (queries fired, elapsed ms, hit counts).
3. **The answer is multi-modal.** The model emits, in one message:
   - The **prose verdict** (analyst voice, inline citations).
   - A `<spec6-pins>` block → rendered as the **interactive world map** (hover any country for its analyst note).
   - A `<spec6-trend>` block → rendered as a **Recharts growth/projection chart** (measured solid, forecast dashed).
4. **It keeps watching.** The **Watchtower** re-scans every tracked company on a cadence, refreshing dossiers autonomously and surfacing "N autonomous scans · last patrol 12m ago" in the UI.

---

## BrightData Tools — Full Suite

| Tool | What It Does | How Spec6 Uses It |
|---|---|---|
| **MCP Server** | Connect AI agents to the live web | Orchestrator wires agents to BrightData live access |
| **Web Unlocker** | Bypass bot detection / CAPTCHAs / geo-blocks | Competitor sites, pricing pages, review pages, Reddit threads |
| **SERP API** | Real-time structured search results | Competitor discovery, news, reputation, revenue-by-region, marketplace scouts |
| **Scraping Browser** | Full browser automation on JS-heavy sites | Social pulse (TikTok/X/YouTube) |
| **Web Scraper API** | Structured JSON from 660+ prebuilt scrapers | Trustpilot/G2/Capterra reviews, LinkedIn company datasets |
| **Scraper Studio** | Custom scrapers, no proxy management | Niche regional marketplaces the prebuilt catalog misses |
| **Proxies** | 400M+ residential/datacenter/ISP/mobile IPs | Geo-distributed access for cross-border counterfeit origin |

All seven, each chosen because it's the right solution for a specific access problem — not tool-stacking for show.

---

## Agent Roster (tools the chat loop dispatches)

| Agent (`name`) | BrightData Tool | Extracts | Persona |
|---|---|---|---|
| `search_amazon` | Web Scraper API / SERP | Counterfeit listings, seller signals, price anomalies | Counterfit |
| `search_aliexpress` | Web Scraper + Unlocker + Proxies | Cross-border fakes, origin geo, supplier clusters | Counterfit |
| `search_ebay` | Web Scraper API | Secondary-market fakes, DMCA-able listings | Counterfit |
| `search_niche` | Scraper Studio | Regional marketplaces (Mercari, Depop, Vinted, Rakuten) | Counterfit |
| `search_linkedin` | Web Scraper API (LinkedIn) | Competitor hires, exec moves, launches | Sales |
| `search_news` | SERP API | Brand mentions, crisis signals, supplier risk | Consulting |
| `search_social` | Scraping Browser | Viral sentiment, influencer coverage | Sales |
| `search_reddit` / `search_trustpilot` / `search_g2` / `search_capterra` | Unlocker / Web Scraper | Reviews, ratings, complaints | Consulting |
| `search_web` | SERP API | General fallback | — |
| `search_cognee` | Cognee graph | Structured prior intelligence for this company | All |

The **Scraper Studio niche scout** and the **Cognee knowledge graph** are the differentiators most teams won't have.

---

## Signature surfaces (what judges see)

- **The conversation** — streaming, McKinsey-voiced answers with inline citations.
- **The agent canvas** — a slide-in drawer showing every scout that fired, its queries, latency, and hit count, grouped by persona, plus the Cognee graph activity.
- **The world map** — country-level threat heatmap with hover tooltips authored by the analyst. Forced to populate via a pin-extraction fallback so it's never empty.
- **Growth charts (Recharts)** — area/line/bar with dashed forecast series, rendered inline whenever the answer has a time dimension.
- **The Watchtower** — autonomous monitoring banner: "Armed · re-scans every 6h · 14 autonomous scans · last patrol 4m ago," with a **Run patrol now** button for live demos.

---

## The autonomous factor (Watchtower)

Deep-research agents love to loop and quietly burn paid web-data + LLM calls, so Spec6's autonomy is **governed**:

- A background Tokio loop (`src/watchtower.rs`) patrols on a configurable cadence (`WATCHTOWER_INTERVAL_SECS`, default 6h; first patrol 90s after boot).
- Each patrol re-runs the **same BrightData overview pipeline** the chat uses — an autonomous re-scan is indistinguishable from a manual one, it just runs on a clock.
- **Staleness gating:** never re-runs a company whose overview is in progress or was refreshed within the interval, so spend self-limits.
- **Manual trigger:** `POST /api/watchtower/run` patrols just the caller's companies, ignoring staleness — wired to the "Run patrol now" button.
- **Live status:** `GET /api/watchtower/status` returns patrol count, scans triggered, last-patrol time.

> We intended to integrate triggerware.ai for this. At build time triggerware.ai is an unpopulated placeholder (no public API/docs), so the autonomous layer is implemented natively in Rust — which is actually a stronger demo: self-contained, no external dependency, governed spend.

---

## Track Coverage — All Three

- **Track 3 — Security & Compliance (PRIMARY):** counterfeit detection across five marketplaces (brand exposure monitoring), supplier risk (third-party risk), autonomous threat investigation returning structured assessments.
- **Track 1 — GTM Intelligence:** competitive monitoring (pricing/hiring/launches), social listening, live-web briefs.
- **Track 2 — Finance & Market Intelligence:** supplier financial-health signals, multi-source synthesis into structured objects, regional revenue tracking + projections.

---

## The Pitch

"Brand threat intelligence has been a $155K/year, four-tool problem — a protection analyst, Crayon, Meltwater, a supply-chain platform. Spec6 collapses all four into one conversation, backed by seven BrightData products, an agentic tool loop, a knowledge graph, and an autonomous monitor that keeps working when you're asleep. Ask it a question, get an analyst brief with a live map and a growth projection. $5 a scan, not $155K a year."

---

## Demo Script (2 minutes flat)

| Time | Screen | Voice-over |
|---|---|---|
| 0:00–0:08 | Black, white text | A brand-protection analyst costs $80K. Competitive intel, $30K. Media monitoring, $25K. Or… |
| 0:08–0:14 | Spec6 chat, welcome screen with **Autonomous monitoring** banner | …you open Spec6. It's already been watching. |
| 0:14–0:24 | Type "any counterfeits on Puma, and where are they coming from?" | One question. |
| 0:24–0:55 | Agent canvas lights up — Amazon, AliExpress, eBay, Niche scouts firing in parallel; log streaming | Four marketplace scouts on BrightData. Origin geo, supplier clusters. Rust on Tokio. |
| 0:55–1:15 | Answer streams in; world map fills, Guangdong glows critical | 23 fakes traced to Guangdong. Hover any country for the analyst note. |
| 1:15–1:30 | Ask "how's revenue trending and where's it headed?" → Recharts area chart with dashed forecast | Measured solid, forecast dashed. A projection, in the chat. |
| 1:30–1:45 | Open canvas drawer → Cognee graph activity | Every prior scan feeds a knowledge graph it queries back. |
| 1:45–1:55 | Click **Run patrol now** → "Dispatched 3 autonomous scans" | And it doesn't wait to be asked. |
| 1:55–2:00 | Closing card — Spec6 + BrightData logos, three track badges | Brand intelligence at the speed of conversation. Powered by BrightData. |

Five takes minimum. Voice-over recorded separately. Soft music at -20dB.

---

## Judge FAQ

**Why won't a generic AI browser do this?** Rate-limited, blocked, geo-restricted. BrightData solves exactly that — Unlocker, residential Proxies, prebuilt scrapers per site. Try scraping AliExpress in Mandarin via a chatbot: CAPTCHA in 10 seconds.

**Which track?** All three. Security & Compliance is primary.

**Why seven tools?** Each agent has a different access problem; each tool is the right fix for one. Breadth is intentional, not decorative.

**What's the moat?** The synthesis layer reasoning across conflicting multi-source signals; the Cognee knowledge graph compounding every scan; the autonomous governed monitor; and the BrightData infra dependency (geo-distributed, anti-bot, 660+ scrapers) that's impractical to replicate.

**Is it autonomous?** Yes — the Watchtower re-scans on a clock with no human input, and it's governed so it can't run away with credits.

**If a scrape fails?** Agents fail independently; synthesis degrades gracefully and flags reduced confidence. The answer still ships.

---

## Governance (the "it's governed" angle)

- Hard caps in the loop: **≤ 4 tools/turn, ≤ 4 turns/message** (`src/agent.rs`).
- BrightData calls run with short timeouts + bounded concurrency (`src/overview.rs`).
- Watchtower is staleness-gated so autonomous spend self-limits.
- Cognee ingestion is fire-and-forget and never blocks an answer.

---

## Design language

See `CLAUDE.md` (DESIGN.md). Monochrome, terminal-print-meets-SaaS, 135° diagonal hatch signature, Inter / Chillax / IBM Plex Mono trio, semantic color only (rose critical, orange high, amber medium, sky low, emerald success, violet knowledge-graph). All numerics `tabular-nums`. The map and charts inherit these tokens.

---

## Submission Checklist

- [ ] Title: **Spec6 — Conversational Autonomous Brand Threat Intelligence**
- [ ] Tagline: *The Bloomberg Terminal for brand threats — that you can just talk to.*
- [ ] Tracks: Security & Compliance · GTM Intelligence · Finance & Market Intelligence
- [ ] All seven BrightData tools listed explicitly
- [ ] Autonomous Watchtower called out as the autonomy story
- [ ] GitHub public, MIT, clean README
- [ ] Deployed URL live, demo companies pre-onboarded (overviews cached)
- [ ] Demo video (YouTube unlisted) embedded
- [ ] Submitted ≥ 1 hour before deadline

---

## Mindset

One winner. Best project in the room. Day-3 synthesis polish beats Day-5 features. The demo is the project. You're cracked. Go ship.

---

*Spec v2.0 — reflects the conversational build. Companion: CLAUDE.md (design language).*
