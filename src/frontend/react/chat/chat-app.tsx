import "katex/dist/katex.min.css";
import { useCallback, useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { MetaFooter } from "../components/footer";
import { useErrorStore } from "../lib/error-store";
import type {
  AuthUser,
  ChatGroup,
  ChatMessage,
  CompanyOverview,
  Conversation,
  InferenceCatalog,
} from "../types";
import { ChatHeader } from "./header";
import { GroupEditorPane } from "./onboarding";
import { SearchPalette } from "./search-palette";
import { Sidebar } from "./sidebar";
import {
  StreamStoppedError,
  consumeMessageWebSocket,
  streamTimingFields,
} from "./stream";
import { ThreadArea } from "./thread";
import { VoiceAssistant } from "./voice-assistant";
import type {
  OverviewActivityEvent,
  OverviewActivityLog,
  StreamTimingPayload,
  ToolCallItem,
  ToolCallStatus,
  ToolCallsByMessage,
} from "./types";
import {
  debugStream,
  dedupeMessages,
  logStreamTiming,
  quickTitleFromMessage,
  selectInferenceChoice,
} from "./utils";

export function ChatApp({
  user,
  initialConversationId,
}: {
  user: AuthUser;
  initialConversationId: string | null;
}) {
  const { pushError } = useErrorStore();

  /* conversations */
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [activeId, setActiveId] = useState<string | null>(initialConversationId);
  const [messages, setMessages] = useState<ChatMessage[]>([]);

  /* groups */
  const [chatGroups, setChatGroups] = useState<ChatGroup[]>([]);
  const [editingGroupId, setEditingGroupId] = useState<string | null>(null);
  const [groupOverviews, setGroupOverviews] = useState<
    Record<string, CompanyOverview | null>
  >({});
  const [groupActivity, setGroupActivity] = useState<OverviewActivityLog>({});
  const [savingGroup, setSavingGroup] = useState(false);

  /* inference */
  const [catalog, setCatalog] = useState<InferenceCatalog | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(true);
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null);
  const [selectedModelId, setSelectedModelId] = useState("");

  /* loading */
  const [loadingConvo, setLoadingConvo] = useState(false);
  const [pendingReply, setPendingReply] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /* tool calls */
  const [liveToolCalls, setLiveToolCalls] = useState<ToolCallItem[]>([]);
  const [toolCallsByMessage, setToolCallsByMessage] = useState<ToolCallsByMessage>({});

  /* ui */
  const [mobileOpen, setMobileOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [voiceOpen, setVoiceOpen] = useState(false);
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    if (typeof window === "undefined") return false;
    return window.localStorage.getItem("ww:sidebar-collapsed") === "1";
  });
  const [composerBody, setComposerBody] = useState("");

  /* refs */
  const skipNextLoadRef = useRef(false);
  const activeIdRef = useRef(activeId);
  const streamingConversationRef = useRef<string | null>(null);
  const activeStreamSocketRef = useRef<WebSocket | null>(null);
  const streamStopRequestedRef = useRef(false);
  const messageCacheRef = useRef<Record<string, ChatMessage[]>>({});

  useEffect(() => { activeIdRef.current = activeId; }, [activeId]);

  useEffect(() => {
    if (typeof window === "undefined") return;
    window.localStorage.setItem("ww:sidebar-collapsed", collapsed ? "1" : "0");
  }, [collapsed]);

  /* ⌘K palette */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  /* load catalog */
  useEffect(() => {
    let cancelled = false;
    setCatalogLoading(true);
    (async () => {
      try {
        const res = await fetch("/api/inference/catalog", {
          credentials: "include",
        });
        if (!res.ok) throw new Error(`catalog ${res.status}`);
        const data = (await res.json()) as InferenceCatalog;
        if (cancelled) return;
        setCatalog(data);
        const next = selectInferenceChoice(data, null, "");
        setSelectedProviderId(next.providerId);
        setSelectedModelId(next.modelId);
      } catch (err) {
        if (!cancelled)
          setError(err instanceof Error ? err.message : "Could not load models");
      } finally {
        if (!cancelled) setCatalogLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, []);

  /* load groups */
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch("/api/chat-groups", { credentials: "include" });
        if (!res.ok) return;
        const data = (await res.json()) as { groups: ChatGroup[] };
        if (!cancelled) setChatGroups(data.groups);
      } catch { /* non-fatal */ }
    })();
    return () => { cancelled = true; };
  }, []);

  /* load conversations */
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch("/api/conversations", { credentials: "include" });
        if (!res.ok) throw new Error(`list ${res.status}`);
        const data = (await res.json()) as { conversations: Conversation[] };
        if (!cancelled) setConversations(data.conversations);
      } catch (err) {
        if (!cancelled)
          setError(err instanceof Error ? err.message : "Could not load chats");
      }
    })();
    return () => { cancelled = true; };
  }, []);

  /* load active conversation */
  useEffect(() => {
    if (!activeId) { setMessages([]); return; }
    if (skipNextLoadRef.current) { skipNextLoadRef.current = false; return; }
    let cancelled = false;
    setLoadingConvo(true);
    (async () => {
      try {
        const res = await fetch(`/api/conversations/${activeId}`, {
          credentials: "include",
        });
        if (res.status === 404) {
          if (!cancelled) {
            setActiveId(null);
            history.replaceState(null, "", "/chat");
          }
          return;
        }
        if (!res.ok) throw new Error(`load ${res.status}`);
        const data = (await res.json()) as {
          conversation: Conversation;
          messages: ChatMessage[];
        };
        if (cancelled) return;
        messageCacheRef.current[activeId] = data.messages;
        if (activeIdRef.current === activeId) setMessages(data.messages);
        setConversations((prev) =>
          prev.map((c) => (c.id === data.conversation.id ? data.conversation : c)),
        );
      } catch (err) {
        if (!cancelled)
          setError(err instanceof Error ? err.message : "Could not load chat");
      } finally {
        if (!cancelled) setLoadingConvo(false);
      }
    })();
    return () => { cancelled = true; };
  }, [activeId]);

  /* keep catalog selection valid */
  useEffect(() => {
    if (!catalog) return;
    const next = selectInferenceChoice(catalog, selectedProviderId, selectedModelId);
    if (next.providerId !== selectedProviderId) setSelectedProviderId(next.providerId);
    if (next.modelId !== selectedModelId) setSelectedModelId(next.modelId);
  }, [catalog, selectedProviderId, selectedModelId]);

  /* shared overview subscription across editor + chat views.
     One EventSource per groupId; auto-closes when the run finishes. */
  const overviewSubsRef = useRef<Record<string, EventSource>>({});

  const pushActivity = useCallback((groupId: string, event: OverviewActivityEvent) => {
    setGroupActivity((prev) => {
      const list = prev[groupId] ?? [];
      // Cap log length to keep memory bounded
      const next = [...list.slice(-99), event];
      return { ...prev, [groupId]: next };
    });
  }, []);

  const subscribeToOverview = useCallback(
    async (groupId: string) => {
      const reload = async (): Promise<CompanyOverview | null> => {
        try {
          const r = await fetch(`/api/chat-groups/${groupId}/overview`, {
            credentials: "include",
          });
          if (!r.ok) return null;
          const d = (await r.json()) as { overview: CompanyOverview | null };
          setGroupOverviews((prev) => ({ ...prev, [groupId]: d.overview }));
          return d.overview;
        } catch {
          return null;
        }
      };

      const ov = await reload();
      if (ov?.status !== "queued" && ov?.status !== "running") return;
      if (overviewSubsRef.current[groupId]) return;

      const es = new EventSource(`/api/chat-groups/${groupId}/overview/stream`);
      overviewSubsRef.current[groupId] = es;
      const closeAndClear = () => {
        es.close();
        delete overviewSubsRef.current[groupId];
      };

      const parse = (raw: string): Record<string, unknown> | null => {
        try { return JSON.parse(raw); } catch { return null; }
      };

      es.addEventListener("overview_status", (e) => {
        const data = parse((e as MessageEvent).data);
        if (!data) return;
        reload();
      });
      es.addEventListener("source_started", (e) => {
        const data = parse((e as MessageEvent).data);
        if (!data) return;
        pushActivity(groupId, {
          kind: "source_started",
          at: Date.now(),
          source: String(data.source ?? "serp"),
          detail: String(data.detail ?? ""),
        });
      });
      es.addEventListener("source_completed", (e) => {
        const data = parse((e as MessageEvent).data);
        if (!data) return;
        pushActivity(groupId, {
          kind: "source_completed",
          at: Date.now(),
          source: String(data.source ?? "serp"),
          detail: String(data.detail ?? ""),
          found: typeof data.found === "number" ? data.found : undefined,
        });
      });
      es.addEventListener("competitor_found", (e) => {
        const data = parse((e as MessageEvent).data);
        if (!data) return;
        const comp = (data.competitor ?? {}) as Record<string, unknown>;
        if (typeof comp.name === "string") {
          pushActivity(groupId, {
            kind: "competitor_found",
            at: Date.now(),
            name: comp.name,
            domain: (comp.domain as string | null | undefined) ?? null,
          });
        }
        reload();
      });
      es.addEventListener("overview_complete", () => {
        reload();
        closeAndClear();
      });
      es.addEventListener("overview_error", () => {
        reload();
        closeAndClear();
      });
      es.onerror = closeAndClear;
    },
    [pushActivity],
  );

  /* close all subscriptions on unmount */
  useEffect(() => {
    return () => {
      for (const es of Object.values(overviewSubsRef.current)) es.close();
      overviewSubsRef.current = {};
    };
  }, []);

  /* whenever the user opens an editor or a chat tied to a company, load that
     company's overview (and subscribe if it's still in progress). */
  const activeConversationGroupIdForEffect = activeId
    ? (conversations.find((c) => c.id === activeId)?.group_id ?? null)
    : null;
  useEffect(() => {
    if (editingGroupId) subscribeToOverview(editingGroupId);
  }, [editingGroupId, subscribeToOverview]);
  useEffect(() => {
    if (activeConversationGroupIdForEffect)
      subscribeToOverview(activeConversationGroupIdForEffect);
  }, [activeConversationGroupIdForEffect, subscribeToOverview]);

  /* ── conversation helpers ── */

  const updateConversationMessages = useCallback(
    (conversationId: string, updater: (prev: ChatMessage[]) => ChatMessage[]) => {
      const previous = messageCacheRef.current[conversationId] ?? [];
      const next = updater(previous);
      messageCacheRef.current[conversationId] = next;
      if (activeIdRef.current === conversationId) setMessages(next);
    },
    [],
  );

  const selectConversation = useCallback((id: string | null) => {
    activeIdRef.current = id;
    setActiveId(id);
    setEditingGroupId(null);
    if (id && messageCacheRef.current[id]) setMessages(messageCacheRef.current[id]);
    if (!id) setMessages([]);
    setMobileOpen(false);
    history.pushState(null, "", id ? `/chat/${id}` : "/chat");
  }, []);

  const newConversation = useCallback(
    async (groupId?: string | null): Promise<string | null> => {
      setMobileOpen(false);
      if (!groupId) {
        setEditingGroupId(null);
        selectConversation(null);
        return null;
      }
      try {
        const res = await fetch("/api/conversations", {
          method: "POST",
          credentials: "include",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ group_id: groupId }),
        });
        if (!res.ok) throw new Error(`create ${res.status}`);
        const data = (await res.json()) as { conversation: Conversation };
        const convo = data.conversation;
        setConversations((prev) => [convo, ...prev]);
        skipNextLoadRef.current = true;
        messageCacheRef.current[convo.id] = [];
        activeIdRef.current = convo.id;
        setActiveId(convo.id);
        setEditingGroupId(null);
        setMessages([]);
        history.pushState(null, "", `/chat/${convo.id}`);
        return convo.id;
      } catch (err) {
        pushError(err instanceof Error ? err.message : "Couldn't create chat");
        return null;
      }
    },
    [selectConversation, pushError],
  );

  const renameConversation = useCallback(async (id: string, nextTitle: string) => {
    const trimmed = nextTitle.trim();
    if (!trimmed) return;
    try {
      const res = await fetch(`/api/conversations/${id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ title: trimmed }),
      });
      if (!res.ok && res.status !== 204) throw new Error(`rename ${res.status}`);
      setConversations((prev) =>
        prev.map((c) =>
          c.id === id ? { ...c, title: trimmed, updated_at: new Date().toISOString() } : c,
        ),
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : "Couldn't rename");
    }
  }, []);

  const deleteConversation = useCallback(
    async (id: string) => {
      if (!confirm("Delete this chat? This can't be undone.")) return;
      try {
        const res = await fetch(`/api/conversations/${id}`, {
          method: "DELETE",
          credentials: "include",
        });
        if (!res.ok && res.status !== 204) throw new Error(`delete ${res.status}`);
        setConversations((prev) => prev.filter((c) => c.id !== id));
        if (activeId === id) selectConversation(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Couldn't delete");
      }
    },
    [activeId, selectConversation],
  );

  /* ── group helpers ── */

  const openGroupEditor = useCallback((groupId: string) => {
    setEditingGroupId(groupId);
    setActiveId(null);
    activeIdRef.current = null;
    setMessages([]);
    history.pushState(null, "", "/chat");
  }, []);

  const createGroup = useCallback(async () => {
    setSavingGroup(true);
    try {
      const res = await fetch("/api/chat-groups", {
        method: "POST",
        credentials: "include",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name: "Untitled company", data_text: "" }),
      });
      if (!res.ok) throw new Error(`create group ${res.status}`);
      const data = (await res.json()) as { group: ChatGroup };
      setChatGroups((prev) => [data.group, ...prev]);
      setEditingGroupId(data.group.id);
      setActiveId(null);
      activeIdRef.current = null;
      setMessages([]);
      history.pushState(null, "", "/chat");
    } catch (err) {
      pushError(err instanceof Error ? err.message : "Couldn't create company");
    } finally {
      setSavingGroup(false);
    }
  }, [pushError]);

  const saveGroup = useCallback(
    async (groupId: string, name: string, data_text: string): Promise<ChatGroup | null> => {
      setSavingGroup(true);
      try {
        const res = await fetch(`/api/chat-groups/${groupId}`, {
          method: "PATCH",
          credentials: "include",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ name: name.trim(), data_text }),
        });
        if (!res.ok) throw new Error(`save group ${res.status}`);
        const data = (await res.json()) as { group: ChatGroup };
        setChatGroups((prev) => prev.map((g) => (g.id === groupId ? data.group : g)));

        // The backend queues an overview on update if the seed is non-empty.
        // Drop any existing SSE subscription, paint an optimistic "queued"
        // placeholder so the loading UI shows immediately, then resubscribe.
        const existingSub = overviewSubsRef.current[groupId];
        if (existingSub) {
          existingSub.close();
          delete overviewSubsRef.current[groupId];
        }
        const nowIso = new Date().toISOString();
        // Reset live activity log for this group — a new run is starting.
        setGroupActivity((prev) => ({ ...prev, [groupId]: [] }));
        setGroupOverviews((prev) => ({
          ...prev,
          [groupId]: {
            company_id: groupId,
            company_name: data.group.name,
            status: "queued",
            started_at: null,
            completed_at: null,
            discovered_competitors: [],
            summary: null,
            markdown_brief: "",
            failure_reason: null,
            created_at: nowIso,
            updated_at: nowIso,
          },
        }));
        subscribeToOverview(groupId);

        return data.group;
      } catch (err) {
        pushError(err instanceof Error ? err.message : "Couldn't save");
        return null;
      } finally {
        setSavingGroup(false);
      }
    },
    [pushError, subscribeToOverview],
  );

  const saveAndStartThread = useCallback(
    async (groupId: string, name: string, data_text: string) => {
      const saved = await saveGroup(groupId, name, data_text);
      if (!saved) return;
      await newConversation(groupId);
    },
    [saveGroup, newConversation],
  );

  const deleteGroup = useCallback(
    async (groupId: string) => {
      if (!confirm("Delete this company and all its data? This can't be undone.")) return;
      try {
        const res = await fetch(`/api/chat-groups/${groupId}`, {
          method: "DELETE",
          credentials: "include",
        });
        if (!res.ok && res.status !== 204) throw new Error(`delete group ${res.status}`);
        setChatGroups((prev) => prev.filter((g) => g.id !== groupId));
        if (editingGroupId === groupId) setEditingGroupId(null);
      } catch (err) {
        pushError(err instanceof Error ? err.message : "Couldn't delete company");
      }
    },
    [editingGroupId, pushError],
  );

  /* ── inference ── */

  const changeProvider = useCallback(
    (providerId: string) => {
      if (!catalog) return;
      const next = selectInferenceChoice(catalog, providerId, "");
      setSelectedProviderId(next.providerId);
      setSelectedModelId(next.modelId);
    },
    [catalog],
  );

  const changeModel = useCallback((modelId: string) => setSelectedModelId(modelId), []);

  /* ── stop streaming ── */

  const stopStreaming = useCallback(() => {
    streamStopRequestedRef.current = true;
    const socket = activeStreamSocketRef.current;
    if (socket && socket.readyState <= WebSocket.OPEN)
      socket.close(4000, "client stop");
    setPendingReply(false);
  }, []);

  /* ── send message ── */

  const sendMessage = useCallback(
    async (body: string) => {
      setError(null);
      if (!selectedProviderId || !selectedModelId) {
        setError("No inference provider is ready yet.");
        return;
      }

      debugStream("send start", { activeId, body });

      const currentGroupId = activeId
        ? (conversations.find((c) => c.id === activeId)?.group_id ?? null)
        : null;

      let convoId = activeId;
      let createdConvo: Conversation | null = null;
      if (!convoId) {
        try {
          const res = await fetch("/api/conversations", {
            method: "POST",
            credentials: "include",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(currentGroupId ? { group_id: currentGroupId } : {}),
          });
          if (!res.ok) throw new Error(`create ${res.status}`);
          const data = (await res.json()) as { conversation: Conversation };
          createdConvo = data.conversation;
          convoId = data.conversation.id;
          // Optimistic title from the first user message — replaces the
          // backend's default "New chat" so the sidebar doesn't sit on a
          // generic label while the agent loop runs. The real title from
          // the backend's `meta` event will override this when it arrives.
          const quick = quickTitleFromMessage(body);
          const seeded =
            quick && data.conversation.title === "New chat"
              ? { ...data.conversation, title: quick }
              : data.conversation;
          setConversations((prev) => [seeded, ...prev]);
          skipNextLoadRef.current = true;
          activeIdRef.current = convoId;
          setActiveId(convoId);
          history.pushState(null, "", `/chat/${convoId}`);
        } catch (err) {
          setError(err instanceof Error ? err.message : "Couldn't start a chat");
          return;
        }
      }

      setLiveToolCalls([]);

      const optimistic: ChatMessage = {
        id: `tmp-${Date.now()}`,
        role: "user",
        body,
        created_at: new Date().toISOString(),
      };
      const assistantTempId = `tmp-assistant-${Date.now()}`;
      streamingConversationRef.current = convoId;
      if (!messageCacheRef.current[convoId]) {
        messageCacheRef.current[convoId] =
          activeIdRef.current === convoId ? messages : [];
      }
      updateConversationMessages(convoId, (prev) => [
        ...prev,
        optimistic,
        {
          id: assistantTempId,
          role: "assistant",
          body: "",
          created_at: new Date().toISOString(),
        },
      ]);
      setPendingReply(true);
      streamStopRequestedRef.current = false;
      let streamCompleted = false;

      const appendAssistantDelta = (delta: string, timing?: StreamTimingPayload) => {
        if (!delta) return;
        logStreamTiming("token append", {
          convoId,
          deltaLength: delta.length,
          ...streamTimingFields(timing),
        });
        if (activeIdRef.current === convoId) {
          flushSync(() => {
            setMessages((prev) => {
              const idx = prev.findIndex((m) => m.id === assistantTempId);
              if (idx === -1) return prev;
              const next = prev.slice();
              next[idx] = { ...next[idx], body: next[idx].body + delta };
              return next;
            });
          });
        }
        const cached = messageCacheRef.current[convoId];
        if (cached) {
          const idx = cached.findIndex((m) => m.id === assistantTempId);
          if (idx !== -1) {
            const updated = cached.slice();
            updated[idx] = { ...updated[idx], body: updated[idx].body + delta };
            messageCacheRef.current[convoId] = updated;
          }
        }
      };

      try {
        await consumeMessageWebSocket(
          convoId,
          { body, provider: selectedProviderId, model: selectedModelId },
          {
            onSocket: (socket) => { activeStreamSocketRef.current = socket; },
            onMeta: (title) => {
              setConversations((prev) =>
                prev.map((c) =>
                  c.id === convoId
                    ? { ...c, title, updated_at: new Date().toISOString() }
                    : c,
                ),
              );
            },
            onToken: (payload) => {
              const delta = payload.delta ?? "";
              if (!delta) return;
              appendAssistantDelta(delta, payload);
            },
            onToolStarted: (callId, toolName, query) => {
              setLiveToolCalls((prev) => [
                ...prev,
                { callId, toolName, query, status: "running", startedAt: Date.now() },
              ]);
            },
            onToolCompleted: (callId, sourceType, resultCount, errored) => {
              setLiveToolCalls((prev) =>
                prev.map((t) =>
                  t.callId === callId
                    ? {
                        ...t,
                        status: "done" as ToolCallStatus,
                        sourceType,
                        endedAt: Date.now(),
                        resultCount,
                        errored,
                      }
                    : t,
                ),
              );
            },
            onDone: (data) => {
              streamCompleted = true;
              setLiveToolCalls((prev) => {
                if (prev.length > 0) {
                  setToolCallsByMessage((byMsg) => ({
                    ...byMsg,
                    [data.assistant.id]: prev,
                  }));
                }
                return [];
              });
              updateConversationMessages(convoId, (prev) =>
                dedupeMessages([
                  ...prev.filter(
                    (m) => m.id !== optimistic.id && m.id !== assistantTempId,
                  ),
                  data.user,
                  data.assistant,
                ]),
              );
              setConversations((prev) =>
                prev.map((c) =>
                  c.id === convoId
                    ? {
                        ...c,
                        title: data.title ?? c.title,
                        updated_at: new Date().toISOString(),
                      }
                    : c,
                ),
              );
            },
            onError: (message) => { throw new Error(message); },
          },
        );
        if (!streamCompleted)
          throw new Error("assistant stream ended before completion");
      } catch (err) {
        if (err instanceof StreamStoppedError || streamStopRequestedRef.current) {
          debugStream("stream stopped", { convoId });
          return;
        }
        updateConversationMessages(convoId, (prev) => {
          const streamedAssistant = prev.find((m) => m.id === assistantTempId);
          return prev.filter((m) => {
            if (m.id === assistantTempId) return !!streamedAssistant?.body.trim();
            return m.id !== optimistic.id;
          });
        });
        if (createdConvo) {
          setConversations((prev) =>
            prev.map((c) =>
              c.id === convoId
                ? { ...c, updated_at: new Date().toISOString() }
                : c,
            ),
          );
        }
        setError(err instanceof Error ? err.message : "Couldn't send");
        setLiveToolCalls([]);
      } finally {
        if (activeStreamSocketRef.current) activeStreamSocketRef.current = null;
        streamStopRequestedRef.current = false;
        if (streamingConversationRef.current === convoId)
          streamingConversationRef.current = null;
        setPendingReply(false);
      }
    },
    [
      activeId,
      conversations,
      messages,
      selectedModelId,
      selectedProviderId,
      updateConversationMessages,
    ],
  );

  /* ── derived ── */

  const activeConversationGroupId = activeId
    ? (conversations.find((c) => c.id === activeId)?.group_id ?? null)
    : null;
  const activeCompany = activeConversationGroupId
    ? (chatGroups.find((g) => g.id === activeConversationGroupId) ?? null)
    : null;
  const activeCompanyOverview = activeConversationGroupId
    ? (groupOverviews[activeConversationGroupId] ?? null)
    : null;
  const activeCompanyActivity: OverviewActivityEvent[] = activeConversationGroupId
    ? (groupActivity[activeConversationGroupId] ?? [])
    : [];
  const editingGroup = editingGroupId
    ? (chatGroups.find((g) => g.id === editingGroupId) ?? null)
    : null;
  const showEditor = !!editingGroupId && !activeId;

  const pickerProps = {
    catalog,
    catalogLoading,
    selectedProviderId,
    selectedModelId,
    onProviderChange: changeProvider,
    onModelChange: changeModel,
  };
  const voiceInferenceChoice = catalog
    ? selectInferenceChoice(catalog, selectedProviderId, selectedModelId)
    : { providerId: selectedProviderId, modelId: selectedModelId };

  return (
    <div className="app-container flex h-screen w-screen overflow-hidden bg-background text-foreground">
      {mobileOpen && (
        <button
          type="button"
          aria-label="Close sidebar"
          onClick={() => setMobileOpen(false)}
          className="fixed inset-0 z-30 bg-black/50 md:hidden"
        />
      )}

      <Sidebar
        user={user}
        conversations={conversations}
        chatGroups={chatGroups}
        groupOverviews={groupOverviews}
        activeId={activeId}
        editingGroupId={editingGroupId}
        onSelectConversation={selectConversation}
        onNewChat={(gid) => { newConversation(gid); }}
        onNewCompany={createGroup}
        onOpenGroupEditor={openGroupEditor}
        onDeleteConversation={deleteConversation}
        onDeleteGroup={deleteGroup}
        mobileOpen={mobileOpen}
        onCloseMobile={() => setMobileOpen(false)}
        collapsed={collapsed}
        onToggleCollapsed={() => setCollapsed((v) => !v)}
        onOpenPalette={() => setPaletteOpen(true)}
      />

      <main className="relative flex h-full min-w-0 flex-1 flex-col overflow-hidden">
        {!showEditor && (
          <ChatHeader
            conversation={conversations.find((c) => c.id === activeId) ?? null}
            onOpenMobile={() => setMobileOpen(true)}
            onRename={renameConversation}
            onDelete={deleteConversation}
            onOpenVoice={() => setVoiceOpen(true)}
          />
        )}

        {showEditor && editingGroup ? (
          <GroupEditorPane
            group={editingGroup}
            overview={groupOverviews[editingGroupId!] ?? null}
            saving={savingGroup}
            onSave={async (name, data_text) => {
              await saveGroup(editingGroup.id, name, data_text);
            }}
            onSaveAndStartThread={(name, data_text) =>
              saveAndStartThread(editingGroup.id, name, data_text)
            }
            onDelete={() => deleteGroup(editingGroup.id)}
            onOpenMobile={() => setMobileOpen(true)}
          />
        ) : (
          <ThreadArea
            activeId={activeId}
            messages={messages}
            loading={loadingConvo}
            pendingReply={pendingReply}
            onSend={sendMessage}
            onStop={stopStreaming}
            error={error}
            user={user}
            composerBody={composerBody}
            onComposerBodyChange={setComposerBody}
            picker={pickerProps}
            chatGroups={chatGroups}
            activeCompany={activeCompany}
            activeCompanyOverview={activeCompanyOverview}
            activeCompanyActivity={activeCompanyActivity}
            liveToolCalls={liveToolCalls}
            toolCallsByMessage={toolCallsByMessage}
            onSelectGroup={(groupId) => { newConversation(groupId); }}
          />
        )}

        <MetaFooter className="mx-3 mt-0" />
      </main>

      {voiceOpen && (
        <VoiceAssistant
          user={user}
          company={activeCompany}
          overview={activeCompanyOverview}
          provider={voiceInferenceChoice.providerId}
          model={voiceInferenceChoice.modelId}
          onClose={() => setVoiceOpen(false)}
        />
      )}

      {paletteOpen && (
        <SearchPalette
          conversations={conversations}
          activeId={activeId}
          onSelect={(id) => {
            selectConversation(id);
            setPaletteOpen(false);
          }}
          onClose={() => setPaletteOpen(false)}
        />
      )}
    </div>
  );
}
