import type { ChatMessage } from "../types";

/* ─── stream payload types ───────────────────────────────────────────────── */

export interface StreamTimingPayload {
  server_event_id?: number;
  server_sent_at_unix_ns?: string;
  server_sent_at_unix_ms?: string;
}

export interface TokenStreamPayload extends StreamTimingPayload {
  delta?: string;
}

export interface DoneStreamPayload extends StreamTimingPayload {
  user: ChatMessage;
  assistant: ChatMessage;
  title: string | null;
}

/**
 * Wire-level shape of any frame the chat WebSocket sends. Field names mirror
 * the backend serialization exactly — `id`/`name` for tool events (NOT
 * `call_id`/`tool_name`), and `title` for meta events. See
 * `src/routes/api.rs` `ws_event` + `agent::ToolStartedEvent` /
 * `agent::ToolCompletedEvent` for the source of truth.
 */
export interface ChatWebSocketPayload extends StreamTimingPayload {
  type?:
    | "token"
    | "meta"
    | "done"
    | "error"
    | "tool_started"
    | "tool_completed";
  delta?: string;
  title?: string | null;
  user?: ChatMessage;
  assistant?: ChatMessage;
  error?: string;
  server_transport?: string;
  /* tool events (both started + completed) */
  id?: string;
  name?: string;
  query?: string;
  source_type?: string;
  label?: string;
  iteration?: number;
  started_at_unix_ms?: number;
  result_count?: number;
  elapsed_ms?: number;
  status?: string;
}

/* ─── tool call ─────────────────────────────────────────────────────────── */

export type ToolCallStatus = "running" | "done";

export interface ToolCallItem {
  callId: string;
  toolName: string;
  query: string;
  status: ToolCallStatus;
  sourceType?: string;
  startedAt: number;
  endedAt?: number;
  resultCount?: number;
  errored?: boolean;
}

export type ToolCallsByMessage = Record<string, ToolCallItem[]>;

/* ─── overview live activity log ────────────────────────────────────────── */

/**
 * Live timeline of what the overview pipeline is doing, populated from the
 * `/api/chat-groups/:id/overview/stream` SSE feed. Used by the "researching"
 * card to show the user what we're currently looking up.
 *
 * Wire event names from src/overview.rs:
 *   overview_status, overview_error, source_started, source_completed,
 *   competitor_found, overview_complete
 */
export type OverviewActivityEvent =
  | { kind: "source_started";   at: number; source: string; detail: string }
  | { kind: "source_completed"; at: number; source: string; detail: string; found?: number }
  | { kind: "competitor_found"; at: number; name: string; domain?: string | null };

export type OverviewActivityLog = Record<string, OverviewActivityEvent[]>;

/* ─── inference picker props ────────────────────────────────────────────── */

import type { InferenceCatalog } from "../types";

export type PickerProps = {
  catalog: InferenceCatalog | null;
  catalogLoading: boolean;
  selectedProviderId: string | null;
  selectedModelId: string;
  onProviderChange: (providerId: string) => void;
  onModelChange: (modelId: string) => void;
};
