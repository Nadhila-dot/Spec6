import type { ChatMessage } from "../types";
import type {
  ChatWebSocketPayload,
  DoneStreamPayload,
  StreamTimingPayload,
  TokenStreamPayload,
} from "./types";
import { debugStream, logStreamTiming } from "./utils";

export class StreamStoppedError extends Error {
  constructor() {
    super("stream stopped");
    this.name = "StreamStoppedError";
  }
}

export function streamTimingFields(payload?: StreamTimingPayload) {
  return {
    server_event_id: payload?.server_event_id,
    server_sent_at_unix_ns: payload?.server_sent_at_unix_ns,
    server_sent_at_unix_ms: payload?.server_sent_at_unix_ms,
  };
}

export function consumeMessageWebSocket(
  conversationId: string,
  request: {
    body: string;
    provider: string;
    model: string;
    response_mode?: "voice";
  },
  handlers: {
    onSocket: (socket: WebSocket) => void;
    onMeta: (title: string) => void;
    onToken: (payload: TokenStreamPayload) => void;
    onToolStarted: (callId: string, toolName: string, query: string) => void;
    onToolCompleted: (
      callId: string,
      sourceType: string,
      resultCount: number,
      errored: boolean,
    ) => void;
    onDone: (payload: DoneStreamPayload) => void;
    onError: (message: string) => never;
  },
) {
  return new Promise<void>((resolve, reject) => {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const socket = new WebSocket(
      `${protocol}//${window.location.host}/api/conversations/${conversationId}/messages/ws`,
    );
    let settled = false;

    const finish = (error?: unknown) => {
      if (settled) return;
      settled = true;
      if (
        socket.readyState === WebSocket.OPEN ||
        socket.readyState === WebSocket.CONNECTING
      ) {
        socket.close();
      }
      if (error) {
        reject(error);
        return;
      }
      resolve();
    };

    socket.addEventListener("open", () => {
      logStreamTiming("websocket open", { conversationId });
      handlers.onSocket(socket);
      socket.send(JSON.stringify(request));
    });

    socket.addEventListener("message", (event) => {
      const payload = JSON.parse(event.data as string) as ChatWebSocketPayload;
      logStreamTiming("websocket message", {
        conversationId,
        type: payload.type,
        ...streamTimingFields(payload),
      });

      if (payload.type === "token") {
        handlers.onToken(payload);
        return;
      }
      if (payload.type === "meta") {
        if (payload.title) handlers.onMeta(payload.title);
        return;
      }
      if (payload.type === "tool_started") {
        // Backend sends { id, name, source_type, query, ... } — not call_id/tool_name.
        if (payload.id && payload.name) {
          handlers.onToolStarted(payload.id, payload.name, payload.query ?? "");
        }
        return;
      }
      if (payload.type === "tool_completed") {
        if (payload.id)
          handlers.onToolCompleted(
            payload.id,
            payload.source_type ?? "",
            payload.result_count ?? 0,
            (payload.status ?? "") === "failed",
          );
        return;
      }
      if (payload.type === "done") {
        if (!payload.user || !payload.assistant) {
          finish(new Error("websocket done frame missing saved messages"));
          return;
        }
        handlers.onDone({
          user: payload.user,
          assistant: payload.assistant,
          title: payload.title ?? null,
          ...streamTimingFields(payload),
        });
        finish();
        return;
      }
      if (payload.type === "error") {
        finish(new Error(payload.error ?? "websocket stream failed"));
      }
    });

    socket.addEventListener("error", () => {
      finish(new Error("websocket connection failed"));
    });

    socket.addEventListener("close", (event) => {
      if (!settled) {
        finish(
          event.code === 4000
            ? new StreamStoppedError()
            : new Error("websocket closed before completion"),
        );
      }
    });
  });
}

/* ─── SSE fallback ───────────────────────────────────────────────────────── */

export async function consumeEventStream(
  response: Response,
  handlers: {
    onMeta: (title: string) => void;
    onToken: (payload: TokenStreamPayload) => void;
    onDone: (payload: {
      user: ChatMessage;
      assistant: ChatMessage;
      title: string | null;
    }) => void;
    onError: (message: string) => never;
  },
) {
  const reader = response.body?.getReader();
  if (!reader) throw new Error("stream reader unavailable");

  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    while (true) {
      const match = buffer.match(/\r?\n\r?\n/);
      if (!match || match.index === undefined) break;
      const rawEvent = buffer.slice(0, match.index);
      buffer = buffer.slice(match.index + match[0].length);
      processStreamEvent(rawEvent, handlers);
    }
    await yieldToBrowser();
  }

  buffer += decoder.decode();
  for (const rawEvent of buffer.split(/\r?\n\r?\n/)) {
    if (rawEvent.trim()) processStreamEvent(rawEvent, handlers);
  }
}

async function yieldToBrowser() {
  await new Promise<void>((resolve) => {
    const canUseRaf =
      typeof window !== "undefined" &&
      typeof document !== "undefined" &&
      document.visibilityState === "visible";
    if (canUseRaf) {
      window.requestAnimationFrame(() => globalThis.setTimeout(resolve, 0));
      return;
    }
    globalThis.setTimeout(resolve, 0);
  });
}

function processStreamEvent(
  rawEvent: string,
  handlers: {
    onMeta: (title: string) => void;
    onToken: (payload: TokenStreamPayload) => void;
    onDone: (payload: {
      user: ChatMessage;
      assistant: ChatMessage;
      title: string | null;
    }) => void;
    onError: (message: string) => never;
  },
) {
  let eventName = "message";
  const dataLines: string[] = [];

  for (const rawLine of rawEvent.split("\n")) {
    const line = rawLine.replace(/\r$/, "");
    if (!line) continue;
    if (line.startsWith("event:")) {
      eventName = line.slice(6).trim();
      continue;
    }
    if (line.startsWith("data:")) dataLines.push(line.slice(5).trimStart());
  }

  if (dataLines.length === 0) return;
  const payload = JSON.parse(dataLines.join("\n")) as unknown;

  debugStream("parsed event", { eventName, payload });

  if (eventName === "token") {
    const token = payload as TokenStreamPayload;
    if (token.delta) handlers.onToken(token);
    return;
  }
  if (eventName === "meta") {
    const meta = payload as { title?: string };
    if (meta.title) handlers.onMeta(meta.title);
    return;
  }
  if (eventName === "done") {
    handlers.onDone(
      payload as { user: ChatMessage; assistant: ChatMessage; title: string | null },
    );
    return;
  }
  if (eventName === "error") {
    const ep = payload as { error?: string };
    handlers.onError(ep.error ?? "streaming request failed");
  }
}
