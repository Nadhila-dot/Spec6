/**
 * Spec6 Voice Assistant — a dedicated, full-screen company copilot.
 *
 * Pipeline:
 *   mic → PCM16 @ 16kHz → /api/voice/transcribe/ws (Speechmatics realtime proxy)
 *       → partial / final transcript → existing Spec6 agent loop (chat WS)
 *       → streamed analyst answer → Speechmatics TTS / browser fallback (spoken back)
 *
 * The Speechmatics credential never touches the browser; we only ever speak to
 * our own backend. This is a new *interaction layer* over the same intelligence
 * engine as chat — same company context, Cognee memory, and Bright Data tools.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import { DiagonalAccent, HatchedChip } from "../components/diagonal";
import {
  IconArrowRight,
  IconBolt,
  IconClose,
  IconMic,
  IconMicOff,
  IconSend,
  IconSparkle,
  IconStop,
  IconVolume,
  IconVolumeOff,
} from "../components/icons";
import { cn } from "../lib/cn";
import type { AuthUser, ChatGroup, CompanyOverview } from "../types";
import { consumeMessageWebSocket } from "./stream";
import { sourceTypeLabel, stripFakeToolNarration } from "./utils";

type VoiceStatus =
  | "idle"
  | "connecting"
  | "listening"
  | "thinking"
  | "speaking";

interface VoiceTurn {
  role: "user" | "assistant";
  text: string;
}

const QUICK_PROMPTS = [
  "How are we doing today?",
  "What changed this week?",
  "What should I worry about right now?",
  "Compare us to our top competitors.",
  "Read me the top 3 risks.",
];

export function VoiceAssistant({
  user,
  company,
  overview,
  provider,
  model,
  onClose,
}: {
  user: AuthUser;
  company: ChatGroup | null;
  overview: CompanyOverview | null;
  provider: string | null;
  model: string;
  onClose: () => void;
}) {
  const [status, setStatus] = useState<VoiceStatus>("idle");
  const [, setVoiceAvailable] = useState<boolean | null>(null);
  const [liveTranscript, setLiveTranscript] = useState("");
  const [answer, setAnswer] = useState("");
  const [toolNote, setToolNote] = useState<string | null>(null);
  const [history, setHistory] = useState<VoiceTurn[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [muted, setMuted] = useState(false);
  const [continuous, setContinuous] = useState(false);
  const [typed, setTyped] = useState("");

  /* refs that callbacks read without re-subscribing */
  const voiceWSRef = useRef<WebSocket | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const processorRef = useRef<ScriptProcessorNode | null>(null);
  const sourceRef = useRef<MediaStreamAudioSourceNode | null>(null);
  const finalsRef = useRef<string[]>([]);
  const partialRef = useRef("");
  const finalizedRef = useRef(false);
  const finalizeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const convoIdRef = useRef<string | null>(null);
  const utterRef = useRef<SpeechSynthesisUtterance | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const audioUrlRef = useRef<string | null>(null);
  const ttsAbortRef = useRef<AbortController | null>(null);
  const mutedRef = useRef(muted);
  const continuousRef = useRef(continuous);
  const statusRef = useRef<VoiceStatus>(status);
  const lastAnswerRef = useRef("");

  useEffect(() => { mutedRef.current = muted; }, [muted]);
  useEffect(() => { continuousRef.current = continuous; }, [continuous]);
  useEffect(() => { statusRef.current = status; }, [status]);

  /* probe availability */
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const r = await fetch("/api/voice/status", { credentials: "include" });
        const d = (await r.json()) as { enabled: boolean };
        if (!cancelled) setVoiceAvailable(d.enabled);
      } catch {
        if (!cancelled) setVoiceAvailable(false);
      }
    })();
    return () => { cancelled = true; };
  }, []);

  /* full teardown on unmount */
  useEffect(() => {
    return () => {
      stopAudio();
      closeVoiceWS();
      stopSpeechOutput();
      if (finalizeTimerRef.current) clearTimeout(finalizeTimerRef.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* ── audio plumbing ─────────────────────────────────────────────────── */

  const renderTranscript = useCallback(() => {
    const merged = [finalsRef.current.join(" "), partialRef.current]
      .filter(Boolean)
      .join(" ")
      .trim();
    setLiveTranscript(merged);
  }, []);

  function stopAudio() {
    try { processorRef.current?.disconnect(); } catch { /* noop */ }
    try { sourceRef.current?.disconnect(); } catch { /* noop */ }
    processorRef.current = null;
    sourceRef.current = null;
    const stream = mediaStreamRef.current;
    if (stream) for (const t of stream.getTracks()) t.stop();
    mediaStreamRef.current = null;
    const ctx = audioCtxRef.current;
    if (ctx && ctx.state !== "closed") ctx.close().catch(() => {});
    audioCtxRef.current = null;
  }

  function closeVoiceWS() {
    const ws = voiceWSRef.current;
    voiceWSRef.current = null;
    if (ws && ws.readyState <= WebSocket.OPEN) ws.close();
  }

  const connectVoice = useCallback(
    () =>
      new Promise<void>((resolve, reject) => {
        const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
        const ws = new WebSocket(
          `${proto}//${window.location.host}/api/voice/transcribe/ws`,
        );
        ws.binaryType = "arraybuffer";
        voiceWSRef.current = ws;
        let ready = false;

        ws.onmessage = (ev) => {
          let m: Record<string, string>;
          try { m = JSON.parse(ev.data as string); } catch { return; }
          if (m.type === "ready") {
            ready = true;
            resolve();
          } else if (m.type === "partial") {
            partialRef.current = m.transcript ?? "";
            renderTranscript();
          } else if (m.type === "final") {
            const t = (m.transcript ?? "").trim();
            if (t) finalsRef.current.push(t);
            partialRef.current = "";
            renderTranscript();
          } else if (m.type === "end") {
            finalizeTurn();
          } else if (m.type === "error") {
            setError(m.error ?? "voice error");
            if (!ready) reject(new Error(m.error ?? "voice error"));
          }
        };
        ws.onerror = () => {
          if (!ready) reject(new Error("voice connection failed"));
        };
        ws.onclose = () => {
          if (!ready) reject(new Error("voice connection closed"));
        };
      }),
    [renderTranscript],
  );

  async function startAudio() {
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
    });
    mediaStreamRef.current = stream;
    const Ctx =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext })
        .webkitAudioContext;
    const ctx = new Ctx();
    audioCtxRef.current = ctx;
    const source = ctx.createMediaStreamSource(stream);
    sourceRef.current = source;
    const processor = ctx.createScriptProcessor(4096, 1, 1);
    processorRef.current = processor;
    const inRate = ctx.sampleRate;
    processor.onaudioprocess = (e) => {
      const input = e.inputBuffer.getChannelData(0);
      const buf = downsampleToPCM16(input, inRate, 16000);
      const ws = voiceWSRef.current;
      if (ws && ws.readyState === WebSocket.OPEN) ws.send(buf);
    };
    source.connect(processor);
    processor.connect(ctx.destination);
  }

  /* ── turn lifecycle ─────────────────────────────────────────────────── */

  const startListening = useCallback(async () => {
    if (statusRef.current === "listening" || statusRef.current === "connecting")
      return;
    setError(null);
    finalsRef.current = [];
    partialRef.current = "";
    finalizedRef.current = false;
    setLiveTranscript("");
    setAnswer("");
    stopSpeechOutput();
    setStatus("connecting");
    try {
      await connectVoice();
      await startAudio();
      setStatus("listening");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Couldn't start listening");
      setStatus("idle");
      stopAudio();
      closeVoiceWS();
    }
  }, [connectVoice]);

  const stopListening = useCallback(() => {
    if (statusRef.current !== "listening" && statusRef.current !== "connecting")
      return;
    stopAudio();
    try {
      voiceWSRef.current?.send(JSON.stringify({ type: "stop" }));
    } catch { /* noop */ }
    setStatus("thinking");
    if (finalizeTimerRef.current) clearTimeout(finalizeTimerRef.current);
    finalizeTimerRef.current = setTimeout(() => finalizeTurn(), 1500);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function finalizeTurn() {
    if (finalizedRef.current) return;
    finalizedRef.current = true;
    if (finalizeTimerRef.current) clearTimeout(finalizeTimerRef.current);
    closeVoiceWS();
    const text = [finalsRef.current.join(" "), partialRef.current]
      .filter(Boolean)
      .join(" ")
      .trim();
    finalsRef.current = [];
    partialRef.current = "";
    if (!text) {
      setStatus("idle");
      return;
    }
    void submitTurn(text);
  }

  const ensureConversation = useCallback(async (): Promise<string | null> => {
    if (convoIdRef.current) return convoIdRef.current;
    try {
      const res = await fetch("/api/conversations", {
        method: "POST",
        credentials: "include",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(company ? { group_id: company.id } : {}),
      });
      if (!res.ok) throw new Error(`create ${res.status}`);
      const d = (await res.json()) as { conversation: { id: string } };
      convoIdRef.current = d.conversation.id;
      return d.conversation.id;
    } catch {
      return null;
    }
  }, [company]);

  const submitTurn = useCallback(
    async (text: string) => {
      setStatus("thinking");
      setLiveTranscript(text);
      setHistory((h) => [...h, { role: "user", text }]);
      const convoId = await ensureConversation();
      if (!convoId || !provider || !model) {
        setError("Voice session isn't ready yet.");
        setStatus("idle");
        return;
      }
      setAnswer("");
      let acc = "";
      try {
        await consumeMessageWebSocket(
          convoId,
          { body: text, provider, model, response_mode: "voice" },
          {
            onSocket: () => {},
            onMeta: () => {},
            onToken: (p) => {
              acc += p.delta ?? "";
              setAnswer(stripFakeToolNarration(acc));
            },
            onToolStarted: (_id, name, query) => {
              setToolNote(`${sourceTypeLabel(name)} · ${query}`.slice(0, 80));
            },
            onToolCompleted: () => {},
            onDone: (d) => {
              acc = d.assistant.body;
              setAnswer(stripFakeToolNarration(acc));
            },
            onError: (m) => {
              throw new Error(m);
            },
          },
        );
        setToolNote(null);
        const clean = stripFakeToolNarration(acc);
        setHistory((h) => [...h, { role: "assistant", text: clean }]);
        lastAnswerRef.current = clean;
        speak(clean, true);
      } catch (e) {
        setToolNote(null);
        setError(e instanceof Error ? e.message : "Voice answer failed");
        setStatus("idle");
      }
    },
    [ensureConversation, provider, model],
  );

  /* ── speech output ──────────────────────────────────────────────────── */

  function stopSpeechOutput() {
    ttsAbortRef.current?.abort();
    ttsAbortRef.current = null;
    window.speechSynthesis?.cancel();
    const audio = audioRef.current;
    if (audio) {
      audio.pause();
      audio.src = "";
      audioRef.current = null;
    }
    const url = audioUrlRef.current;
    if (url) {
      URL.revokeObjectURL(url);
      audioUrlRef.current = null;
    }
  }

  function afterSpeak() {
    stopSpeechOutput();
    setStatus("idle");
    if (continuousRef.current && !mutedRef.current) {
      // small gap so the mic doesn't catch the tail of TTS
      setTimeout(() => void startListening(), 350);
    }
  }

  function speakWithBrowser(text: string, full = false) {
    const synth = typeof window !== "undefined" ? window.speechSynthesis : null;
    if (mutedRef.current || !synth) {
      afterSpeak();
      return;
    }
    const spoken = full ? toSpeech(text) : shorten(toSpeech(text));
    if (!spoken) {
      afterSpeak();
      return;
    }
    synth.cancel();
    const u = new SpeechSynthesisUtterance(spoken);
    u.rate = 1.05;
    u.pitch = 1;
    const preferred = synth
      .getVoices()
      .find((v) => /en-US|en-GB/i.test(v.lang) && /Google|Natural|Samantha|Daniel/i.test(v.name));
    if (preferred) u.voice = preferred;
    u.onend = afterSpeak;
    u.onerror = afterSpeak;
    utterRef.current = u;
    setStatus("speaking");
    synth.speak(u);
  }

  async function speakWithSpeechmatics(text: string, full = false) {
    const spoken = full ? toSpeech(text) : shorten(toSpeech(text));
    if (!spoken) {
      afterSpeak();
      return;
    }

    stopSpeechOutput();
    setStatus("speaking");
    const controller = new AbortController();
    ttsAbortRef.current = controller;
    const chunks = full ? splitSpeechChunks(spoken) : [spoken];

    for (const chunk of chunks) {
      if (controller.signal.aborted) return;
      const response = await fetch("/api/voice/tts", {
        method: "POST",
        credentials: "include",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ text: chunk, full: false }),
        signal: controller.signal,
      });

      if (!response.ok) {
        throw new Error(`tts ${response.status}`);
      }

      const blob = await response.blob();
      if (controller.signal.aborted) {
        return;
      }

      await playSpeechBlob(blob, controller, audioRef, audioUrlRef);
    }

    afterSpeak();
  }

  function speak(text: string, full = false) {
    if (mutedRef.current) {
      afterSpeak();
      return;
    }

    void (async () => {
      try {
        await speakWithSpeechmatics(text, full);
      } catch {
        speakWithBrowser(text, full);
      }
    })();
  }

  const stopSpeaking = useCallback(() => {
    stopSpeechOutput();
    setStatus("idle");
  }, []);

  const readFull = useCallback(() => {
    if (lastAnswerRef.current) speak(lastAnswerRef.current, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onMicClick = useCallback(() => {
    if (status === "listening" || status === "connecting") stopListening();
    else if (status === "speaking") stopSpeaking();
    else void startListening();
  }, [status, startListening, stopListening, stopSpeaking]);

  const sendTyped = useCallback(() => {
    const t = typed.trim();
    if (!t) return;
    setTyped("");
    void submitTurn(t);
  }, [typed, submitTurn]);

  /* ── render ─────────────────────────────────────────────────────────── */

  const statusMeta = STATUS_META[status];

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-background">
      {/* top state bar */}
      <header className="relative flex shrink-0 items-center gap-3 overflow-hidden border-b border-border bg-card px-4 py-3">
        <div className="absolute inset-0 bg-gradient-to-br from-zinc-800 via-zinc-900 to-zinc-950" />
        <div className="diagonal-line-corner absolute inset-0" />
        <div className="absolute inset-0 bg-gradient-to-r from-background/40 via-background/5 to-transparent" />
        <div className="relative z-10 flex items-center gap-3">
          <HatchedChip size={36}>
            <IconWaveOrb status={status} />
          </HatchedChip>
          <div>
            <div className="text-[10px] font-bold uppercase tracking-[0.14em] text-white/55">
              Spec6 · Voice Assistant
            </div>
            <div className="font-chillax text-[17px] font-semibold tracking-tight text-white">
              {company?.name ?? "General copilot"}
            </div>
          </div>
        </div>

        <div className="relative z-10 ml-auto flex items-center gap-2">
          <span
            className={cn(
              "inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[10.5px] font-semibold uppercase tracking-[0.12em] ring-1",
              statusMeta.pill,
            )}
          >
            <span className={cn("h-1.5 w-1.5 rounded-full", statusMeta.dot, statusMeta.pulse && "animate-pulse")} />
            {statusMeta.label}
          </span>
          <ToggleButton
            active={continuous}
            onClick={() => setContinuous((v) => !v)}
            title="Continuous listening"
          >
            <IconBolt size={14} />
          </ToggleButton>
          <ToggleButton
            active={!muted}
            onClick={() => setMuted((v) => !v)}
            title={muted ? "Unmute output" : "Mute output"}
          >
            {muted ? <IconVolumeOff size={14} /> : <IconVolume size={14} />}
          </ToggleButton>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close voice assistant"
            className="grid h-9 w-9 place-items-center rounded-lg bg-background/15 text-white/85 ring-1 ring-white/15 transition hover:bg-background/25"
          >
            <IconClose size={15} />
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1 overflow-hidden">
        {/* center pane */}
        <div className="flex min-w-0 flex-1 flex-col overflow-y-auto px-5 py-6 sm:px-8">
          <div className="mx-auto flex w-full max-w-2xl flex-1 flex-col items-center gap-6">
            {error && (
              <div className="w-full rounded-xl bg-red-500/[0.08] px-4 py-2.5 text-[12.5px] text-red-400 ring-1 ring-red-500/25">
                {error}
              </div>
            )}

            {/* mic orb */}
            <button
              type="button"
              onClick={onMicClick}
              className="group relative mt-4 grid h-40 w-40 place-items-center rounded-full"
            >
              <MicOrb status={status} />
              <span className="relative z-10 text-white">
                {status === "listening" || status === "connecting" ? (
                  <IconMicOff size={34} />
                ) : status === "speaking" ? (
                  <IconStop size={32} />
                ) : (
                  <IconMic size={34} />
                )}
              </span>
            </button>
            <p className="text-[12.5px] text-muted-foreground/70">
              {status === "listening"
                ? "Listening… tap to send"
                : status === "connecting"
                  ? "Connecting…"
                  : status === "thinking"
                    ? "Thinking…"
                    : status === "speaking"
                      ? "Speaking… tap to stop"
                      : "Tap to talk to Spec6"}
            </p>

            {/* live transcript */}
            {(liveTranscript || status === "listening") && (
              <div className="w-full rounded-xl border border-border bg-card/60 px-4 py-3">
                <div className="text-[9.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
                  You
                </div>
                <p className="mt-1 text-[15px] leading-[1.55] text-foreground">
                  {liveTranscript || (
                    <span className="text-muted-foreground/40">…</span>
                  )}
                </p>
              </div>
            )}

            {/* answer */}
            {(answer || status === "thinking" || toolNote) && (
              <div className="relative w-full overflow-hidden rounded-xl border border-border bg-card px-4 py-3 shadow-[0_1px_2px_rgba(0,0,0,0.05)]">
                <DiagonalAccent className="text-foreground rounded-xl opacity-[0.03]" />
                <div className="relative flex items-center gap-1.5 text-[9.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
                  <IconSparkle size={11} className="text-muted-foreground/50" />
                  Spec6
                  {toolNote && (
                    <span className="ml-2 inline-flex items-center gap-1 rounded-full bg-emerald-500/10 px-1.5 py-0.5 text-[9px] font-semibold tracking-[0.1em] text-emerald-400 ring-1 ring-emerald-500/20">
                      <span className="h-1 w-1 animate-pulse rounded-full bg-emerald-400" />
                      {toolNote}
                    </span>
                  )}
                </div>
                <div className="relative mt-1 text-[15px] leading-[1.6] text-foreground">
                  {answer ? (
                    <VoiceMarkdown body={answer} />
                  ) : (
                    <span className="text-muted-foreground/40">
                      Pulling your company context…
                    </span>
                  )}
                </div>
                {answer && status !== "speaking" && (
                  <div className="relative mt-2 flex gap-2">
                    <MiniAction onClick={() => speak(lastAnswerRef.current)}>
                      <IconVolume size={12} /> Replay
                    </MiniAction>
                    <MiniAction onClick={readFull}>Read full</MiniAction>
                  </div>
                )}
                {status === "speaking" && (
                  <div className="relative mt-2">
                    <MiniAction onClick={stopSpeaking}>
                      <IconStop size={12} /> Stop speaking
                    </MiniAction>
                  </div>
                )}
              </div>
            )}

            {/* quick prompts */}
            <div className="flex w-full flex-wrap justify-center gap-2">
              {QUICK_PROMPTS.map((p) => (
                <button
                  key={p}
                  type="button"
                  onClick={() => void submitTurn(p)}
                  disabled={status === "thinking"}
                  className="group relative isolate inline-flex items-center gap-1.5 overflow-hidden rounded-full bg-card/60 px-3 py-1.5 text-[12px] font-medium tracking-tight text-muted-foreground/80 ring-1 ring-border/60 transition hover:bg-card hover:text-foreground hover:ring-border disabled:opacity-50"
                >
                  <span
                    aria-hidden
                    className="pointer-events-none absolute inset-0 z-0 rounded-full opacity-[0.04] transition-opacity group-hover:opacity-[0.08]"
                    style={{
                      backgroundImage:
                        "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
                    }}
                  />
                  <span className="relative z-10">{p}</span>
                </button>
              ))}
            </div>

            {/* typed fallback */}
            <div className="mt-auto flex w-full items-center gap-2 pt-4">
              <input
                value={typed}
                onChange={(e) => setTyped(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") sendTyped();
                }}
                placeholder="Or type a question…"
                className="h-10 flex-1 rounded-full bg-card px-4 text-[13px] text-foreground ring-1 ring-border/60 outline-none placeholder:text-muted-foreground/45 focus:ring-border"
              />
              <button
                type="button"
                onClick={sendTyped}
                disabled={!typed.trim()}
                className="grid h-10 w-10 place-items-center rounded-full bg-foreground text-background ring-1 ring-border transition hover:opacity-90 disabled:opacity-40"
              >
                <IconSend size={14} />
              </button>
            </div>
          </div>
        </div>

        {/* side panel */}
        <aside className="hidden w-[320px] shrink-0 flex-col overflow-y-auto border-l border-border bg-shell px-4 py-5 lg:flex">
          <SidePanel company={company} overview={overview} history={history} />
        </aside>
      </div>

      <div className="px-4 pb-2 text-center text-[10px] text-muted-foreground/40">
        Realtime STT + TTS by Speechmatics · {user.display_name}
      </div>
    </div>
  );
}

/* ─── side panel ────────────────────────────────────────────────────────── */

function SidePanel({
  company,
  overview,
  history,
}: {
  company: ChatGroup | null;
  overview: CompanyOverview | null;
  history: VoiceTurn[];
}) {
  const summary = overview?.summary;
  const competitors = overview?.discovered_competitors ?? [];
  return (
    <div className="flex flex-col gap-5">
      <Section label="Company context">
        {company ? (
          <div className="space-y-2">
            {summary?.rating && <ContextRow label="Rating" value={summary.rating} />}
            {summary?.faults && <ContextRow label="Weakness" value={summary.faults} />}
            {summary?.where_to_do_better && (
              <ContextRow label="Improve" value={summary.where_to_do_better} />
            )}
            {!summary && (
              <p className="text-[12px] text-muted-foreground/55">
                {overview?.status === "running" || overview?.status === "queued"
                  ? "Overview is still building…"
                  : "No overview yet — ask a question to research live."}
              </p>
            )}
          </div>
        ) : (
          <p className="text-[12px] text-muted-foreground/55">
            No company selected — open one for grounded answers.
          </p>
        )}
      </Section>

      {competitors.length > 0 && (
        <Section label="Top competitors">
          <div className="flex flex-wrap gap-1.5">
            {competitors.slice(0, 6).map((c, i) => (
              <span
                key={`${c.name}-${i}`}
                className="inline-flex items-center gap-1.5 rounded-full bg-foreground/[0.04] px-2.5 py-1 text-[11.5px] font-medium tracking-tight text-foreground/85 ring-1 ring-border/60"
              >
                <span className="h-1 w-1 rounded-full bg-emerald-400/70" />
                {c.name}
              </span>
            ))}
          </div>
        </Section>
      )}

      <Section label="Conversation">
        {history.length === 0 ? (
          <p className="text-[12px] text-muted-foreground/45">No turns yet.</p>
        ) : (
          <ol className="space-y-2.5">
            {history.slice(-8).map((t, i) => (
              <li key={i} className="text-[12px] leading-[1.5]">
                <span
                  className={cn(
                    "mr-1.5 text-[9px] font-bold uppercase tracking-[0.12em]",
                    t.role === "user"
                      ? "text-muted-foreground/55"
                      : "text-emerald-400/80",
                  )}
                >
                  {t.role === "user" ? "You" : "Spec6"}
                </span>
                <span className="text-foreground/80">
                  {t.text.length > 160 ? t.text.slice(0, 160) + "…" : t.text}
                </span>
              </li>
            ))}
          </ol>
        )}
      </Section>
    </div>
  );
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="mb-2 text-[9.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
        {label}
      </div>
      {children}
    </div>
  );
}

function ContextRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-border/60 bg-card px-3 py-2">
      <div className="text-[9px] font-bold uppercase tracking-[0.13em] text-muted-foreground/50">
        {label}
      </div>
      <div className="mt-0.5 text-[12px] leading-[1.45] text-foreground/85">
        {value.length > 180 ? value.slice(0, 180) + "…" : value}
      </div>
    </div>
  );
}

/* ─── bits ──────────────────────────────────────────────────────────────── */

function MiniAction({
  children,
  onClick,
}: {
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex items-center gap-1 rounded-md bg-foreground/[0.05] px-2 py-1 text-[10.5px] font-semibold text-foreground/75 ring-1 ring-border/60 transition hover:bg-foreground/[0.09] hover:text-foreground"
    >
      {children}
    </button>
  );
}

function VoiceMarkdown({ body }: { body: string }) {
  return (
    <div className="assistant-markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={{
          a: ({ children, ...props }) => (
            <a {...props} target="_blank" rel="noreferrer">
              {children}
            </a>
          ),
        }}
      >
        {body}
      </ReactMarkdown>
    </div>
  );
}

function ToggleButton({
  active,
  onClick,
  title,
  children,
}: {
  active: boolean;
  onClick: () => void;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      className={cn(
        "grid h-9 w-9 place-items-center rounded-lg ring-1 transition",
        active
          ? "bg-emerald-500/15 text-emerald-300 ring-emerald-400/30"
          : "bg-background/15 text-white/70 ring-white/15 hover:bg-background/25",
      )}
    >
      {children}
    </button>
  );
}

function IconWaveOrb({ status }: { status: VoiceStatus }) {
  const active = status === "listening" || status === "speaking";
  const heights = active ? [10, 16, 13, 7] : [6, 6, 6, 6];
  return (
    <span className="inline-flex items-center gap-[2px]">
      {heights.map((h, i) => (
        <span
          key={i}
          className={cn("w-[2px] rounded-full bg-white/85", active && "animate-pulse")}
          style={{ height: h, animationDelay: `${i * 140}ms` }}
        />
      ))}
    </span>
  );
}

function MicOrb({ status }: { status: VoiceStatus }) {
  const tone =
    status === "listening"
      ? "from-emerald-500 to-emerald-700"
      : status === "speaking"
        ? "from-violet-500 to-violet-700"
        : status === "thinking"
          ? "from-amber-500 to-amber-700"
          : "from-zinc-600 to-zinc-800";
  const pulsing = status === "listening" || status === "speaking";
  return (
    <>
      {pulsing && (
        <span
          className={cn(
            "absolute inset-0 rounded-full bg-gradient-to-br opacity-40 animate-ping",
            tone,
          )}
        />
      )}
      <span
        className={cn(
          "absolute inset-0 rounded-full bg-gradient-to-br shadow-[0_12px_48px_-8px_rgba(0,0,0,0.6)] ring-1 ring-white/10",
          tone,
        )}
      />
      <span
        aria-hidden
        className="absolute inset-0 rounded-full opacity-[0.18]"
        style={{
          backgroundImage:
            "repeating-linear-gradient(135deg,rgba(255,255,255,0.9) 0,rgba(255,255,255,0.9) 1px,transparent 1px,transparent 7px)",
        }}
      />
    </>
  );
}

const STATUS_META: Record<
  VoiceStatus,
  { label: string; pill: string; dot: string; pulse: boolean }
> = {
  idle: {
    label: "Idle",
    pill: "bg-white/10 text-white/70 ring-white/15",
    dot: "bg-white/60",
    pulse: false,
  },
  connecting: {
    label: "Connecting",
    pill: "bg-amber-500/15 text-amber-300 ring-amber-400/30",
    dot: "bg-amber-400",
    pulse: true,
  },
  listening: {
    label: "Listening",
    pill: "bg-emerald-500/15 text-emerald-300 ring-emerald-400/30",
    dot: "bg-emerald-400",
    pulse: true,
  },
  thinking: {
    label: "Thinking",
    pill: "bg-amber-500/15 text-amber-300 ring-amber-400/30",
    dot: "bg-amber-400",
    pulse: true,
  },
  speaking: {
    label: "Speaking",
    pill: "bg-violet-500/15 text-violet-300 ring-violet-400/30",
    dot: "bg-violet-400",
    pulse: true,
  },
};

/* ─── audio + text helpers ──────────────────────────────────────────────── */

function downsampleToPCM16(
  input: Float32Array,
  inRate: number,
  outRate: number,
): ArrayBuffer {
  const ratio = inRate / outRate;
  const outLen = Math.max(1, Math.floor(input.length / ratio));
  const out = new Int16Array(outLen);
  for (let i = 0; i < outLen; i++) {
    const idx = Math.floor(i * ratio);
    let s = input[idx] ?? 0;
    s = Math.max(-1, Math.min(1, s));
    out[i] = s < 0 ? s * 0x8000 : s * 0x7fff;
  }
  return out.buffer;
}

/** Markdown → flat speakable text. */
function toSpeech(md: string): string {
  let t = stripFakeToolNarration(md);
  t = t.replace(/```[\s\S]*?```/g, " ");
  t = t.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1");
  t = t.replace(/[#>*_`~|]/g, "");
  t = t.replace(/\n{2,}/g, ". ").replace(/\n/g, " ");
  t = t.replace(/\s+/g, " ").trim();
  return t;
}

/** First couple of sentences, capped — the "concise briefing" default. */
function shorten(s: string, max = 360): string {
  if (s.length <= max) return s;
  const sentences = s.split(/(?<=[.!?])\s+/);
  let out = "";
  for (const sentence of sentences) {
    if ((out + sentence).length > max) break;
    out += sentence + " ";
  }
  return (out.trim() || s.slice(0, max)).trim();
}

function splitSpeechChunks(text: string, max = 850): string[] {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (!normalized) return [];

  const sentences = normalized.split(/(?<=[.!?])\s+/);
  const chunks: string[] = [];
  let current = "";

  for (const sentence of sentences) {
    if (!sentence) continue;
    if (!current) {
      current = sentence;
      continue;
    }
    if ((current + " " + sentence).length <= max) {
      current += " " + sentence;
      continue;
    }
    chunks.push(current.trim());
    current = sentence;
  }

  if (current.trim()) chunks.push(current.trim());
  return chunks.length ? chunks : [normalized.slice(0, max)];
}

function playSpeechBlob(
  blob: Blob,
  controller: AbortController,
  audioRef: { current: HTMLAudioElement | null },
  audioUrlRef: { current: string | null },
): Promise<void> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(blob);
    const audio = new Audio(url);
    audioUrlRef.current = url;
    audioRef.current = audio;

    const cleanup = () => {
      audio.pause();
      audio.src = "";
      if (audioRef.current === audio) audioRef.current = null;
      if (audioUrlRef.current === url) audioUrlRef.current = null;
      URL.revokeObjectURL(url);
    };

    const onAbort = () => {
      cleanup();
      resolve();
    };

    controller.signal.addEventListener("abort", onAbort, { once: true });
    audio.onended = () => {
      controller.signal.removeEventListener("abort", onAbort);
      cleanup();
      resolve();
    };
    audio.onerror = () => {
      controller.signal.removeEventListener("abort", onAbort);
      cleanup();
      reject(new Error("audio playback failed"));
    };

    void audio.play().catch((err) => {
      controller.signal.removeEventListener("abort", onAbort);
      cleanup();
      reject(err);
    });
  });
}
