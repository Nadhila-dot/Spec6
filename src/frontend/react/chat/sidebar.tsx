import { useEffect, useMemo, useRef, useState } from "react";
import {
  IconChat,
  IconChevronDown,
  IconClose,
  IconFolder,
  IconPencil,
  IconPlus,
  IconSearch,
  IconSidebarCollapse,
  IconSidebarExpand,
  IconTrash,
} from "../components/icons";
import { WorkspaceCard } from "../components/workspace-card";
import { cn } from "../lib/cn";
import type { AuthUser, ChatGroup, CompanyOverview, Conversation } from "../types";

export function Sidebar({
  user,
  conversations,
  chatGroups,
  groupOverviews,
  activeId,
  editingGroupId,
  onSelectConversation,
  onNewChat,
  onNewCompany,
  onOpenGroupEditor,
  onDeleteConversation,
  onDeleteGroup,
  mobileOpen,
  onCloseMobile,
  collapsed,
  onToggleCollapsed,
  onOpenPalette,
}: {
  user: AuthUser;
  conversations: Conversation[];
  chatGroups: ChatGroup[];
  groupOverviews: Record<string, CompanyOverview | null>;
  activeId: string | null;
  editingGroupId: string | null;
  onSelectConversation: (id: string | null) => void;
  onNewChat: (groupId?: string | null) => void;
  onNewCompany: () => void;
  onOpenGroupEditor: (id: string) => void;
  onDeleteConversation: (id: string) => void;
  onDeleteGroup: (id: string) => void;
  mobileOpen: boolean;
  onCloseMobile: () => void;
  collapsed: boolean;
  onToggleCollapsed: () => void;
  onOpenPalette: () => void;
}) {
  const isCompact = collapsed && !mobileOpen;
  const [searchQuery, setSearchQuery] = useState("");
  const searchInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isCompact) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        searchInputRef.current?.focus();
        searchInputRef.current?.select();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [isCompact]);

  const q = searchQuery.trim().toLowerCase();

  const filteredConversations = useMemo(
    () =>
      q
        ? conversations.filter((c) => c.title.toLowerCase().includes(q))
        : conversations,
    [conversations, q],
  );

  const convsByGroup = useMemo(() => {
    const map: Record<string, Conversation[]> = {};
    for (const c of conversations) {
      const gid = c.group_id ?? "__none__";
      (map[gid] ??= []).push(c);
    }
    return map;
  }, [conversations]);

  const ungrouped = convsByGroup["__none__"] ?? [];

  return (
    <aside
      className={cn(
        "flex h-full shrink-0 flex-col border-r border-border bg-sidebar text-sidebar-foreground",
        "fixed inset-y-0 left-0 z-40 -translate-x-full md:static md:translate-x-0",
        "transition-[width,transform] duration-200 ease-out",
        mobileOpen && "translate-x-0",
        isCompact ? "w-[64px]" : "w-[280px]",
      )}
    >
      {/* brand row */}
      <div
        className={cn(
          "flex items-center px-2.5 pt-3.5 pb-3",
          isCompact ? "flex-col gap-2" : "justify-between gap-2.5 px-4",
        )}
      >
        <a
          href="/chat"
          onClick={(e) => {
            e.preventDefault();
            onSelectConversation(null);
          }}
          className={cn(
            "flex items-center rounded-lg px-1",
            isCompact && "justify-center",
          )}
          title={isCompact ? "Sentinel" : undefined}
        >
          <span
            className={cn(
              "font-chillax font-semibold tracking-tight text-sidebar-foreground/95",
              isCompact ? "text-[13px]" : "text-[15px]",
            )}
          >
            {isCompact ? "S" : "Sentinel"}
          </span>
        </a>

        <button
          type="button"
          onClick={onCloseMobile}
          aria-label="Close sidebar"
          className={cn(
            "grid h-8 w-8 place-items-center rounded-lg text-muted-foreground/65 hover:bg-card/60 hover:text-foreground md:hidden",
            isCompact && "hidden",
          )}
        >
          <IconClose size={16} />
        </button>

        <button
          type="button"
          onClick={onToggleCollapsed}
          aria-label={isCompact ? "Expand sidebar" : "Collapse sidebar"}
          className="hidden h-8 w-8 place-items-center rounded-lg text-muted-foreground/55 hover:bg-card/60 hover:text-foreground md:grid"
        >
          {isCompact ? <IconSidebarExpand size={16} /> : <IconSidebarCollapse size={16} />}
        </button>
      </div>

      {/* search */}
      {!isCompact && (
        <div className="px-2.5 pb-2">
          <div
            className={cn(
              "group/search flex h-9 items-center gap-2 rounded-full bg-card pl-3 pr-1.5",
              "ring-1 ring-border shadow-[0_1px_2px_rgba(0,0,0,0.05)] transition-shadow focus-within:ring-foreground/30",
            )}
          >
            <IconSearch size={13} className="shrink-0 text-muted-foreground/55" />
            <input
              ref={searchInputRef}
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") {
                  setSearchQuery("");
                  e.currentTarget.blur();
                }
              }}
              placeholder="Search chats"
              className="min-w-0 flex-1 bg-transparent text-[12.5px] text-foreground placeholder:text-muted-foreground/55 outline-none"
            />
            <kbd
              className="hidden h-[18px] shrink-0 cursor-pointer items-center rounded border border-border/70 bg-sidebar-accent/40 px-1.5 font-mono text-[10px] font-semibold text-muted-foreground/60 sm:inline-flex"
              onClick={onOpenPalette}
            >
              ⌘K
            </kbd>
          </div>
        </div>
      )}

      {/* new company */}
      {!isCompact && (
        <div className="px-2.5 pb-2">
          <button
            type="button"
            onClick={onNewCompany}
            className={cn(
              "group relative isolate flex h-9 w-full items-center justify-center gap-2 overflow-hidden rounded-lg",
              "bg-card text-[13px] font-semibold text-foreground",
              "ring-1 ring-border shadow-[0_1px_2px_rgba(0,0,0,0.05)] hover:bg-card/80",
            )}
          >
            <span
              aria-hidden
              className="pointer-events-none absolute inset-0 z-0 rounded-[inherit] opacity-[0.05] transition-opacity group-hover:opacity-[0.09]"
              style={{
                backgroundImage:
                  "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
              }}
            />
            <span className="relative z-10 inline-flex items-center gap-2">
              <IconPlus size={15} />
              New company
            </span>
          </button>
        </div>
      )}

      {isCompact ? (
        <div className="flex-1 overflow-hidden px-2 py-2">
          <div className="flex flex-col items-center gap-1.5">
            {conversations.slice(0, 8).map((c) => (
              <div key={c.id} className="group/tick relative">
                <button
                  type="button"
                  onClick={() => onSelectConversation(c.id)}
                  className={cn(
                    "h-1.5 w-6 rounded-full transition-colors",
                    c.id === activeId
                      ? "bg-foreground"
                      : "bg-muted-foreground/25 hover:bg-muted-foreground/45",
                  )}
                />
                <div className="pointer-events-none absolute left-full top-1/2 z-50 ml-3 -translate-y-1/2 opacity-0 transition-opacity group-hover/tick:opacity-100">
                  <div className="whitespace-nowrap rounded-lg border border-border/60 bg-popover px-2.5 py-1.5 text-[12px] text-popover-foreground shadow-[0_4px_16px_rgba(0,0,0,0.35)]">
                    {c.title}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : (
        <div className="flex flex-1 flex-col overflow-y-auto overflow-x-hidden px-2.5 pb-2">
          {q ? (
            <div className="mt-1">
              {filteredConversations.length === 0 ? (
                <p className="px-2.5 py-3 text-[12.5px] text-muted-foreground/55">
                  No matches.
                </p>
              ) : (
                <div className="ml-[18px] border-l border-foreground/25 pl-4">
                  {filteredConversations.map((c) => (
                    <ConversationRow
                      key={c.id}
                      conversation={c}
                      isActive={c.id === activeId}
                      onClick={() => onSelectConversation(c.id)}
                      onDelete={() => onDeleteConversation(c.id)}
                    />
                  ))}
                </div>
              )}
            </div>
          ) : (
            <>
              {chatGroups.map((group) => (
                <GroupSection
                  key={group.id}
                  group={group}
                  overview={groupOverviews[group.id] ?? null}
                  conversations={convsByGroup[group.id] ?? []}
                  activeId={activeId}
                  isEditing={editingGroupId === group.id}
                  onOpenEditor={() => onOpenGroupEditor(group.id)}
                  onNewChat={() => onNewChat(group.id)}
                  onSelectConversation={onSelectConversation}
                  onDeleteConversation={onDeleteConversation}
                  onDeleteGroup={() => onDeleteGroup(group.id)}
                />
              ))}

              {ungrouped.length > 0 && (
                <UngroupedSection
                  conversations={ungrouped}
                  activeId={activeId}
                  onSelectConversation={onSelectConversation}
                  onDeleteConversation={onDeleteConversation}
                  onNewChat={() => onNewChat(null)}
                />
              )}

              {chatGroups.length === 0 && ungrouped.length === 0 && (
                <p className="px-2.5 py-4 text-[12.5px] text-muted-foreground/45">
                  No chats yet.
                </p>
              )}
            </>
          )}
        </div>
      )}

      <div
        className={cn(
          "border-t border-border pt-2 pb-2.5",
          isCompact ? "px-2" : "px-2.5",
        )}
      >
        <WorkspaceCard user={user} compact={isCompact} />
      </div>
    </aside>
  );
}

function GroupSection({
  group,
  overview,
  conversations,
  activeId,
  isEditing,
  onOpenEditor,
  onNewChat,
  onSelectConversation,
  onDeleteConversation,
  onDeleteGroup,
}: {
  group: ChatGroup;
  overview: CompanyOverview | null;
  conversations: Conversation[];
  activeId: string | null;
  isEditing: boolean;
  onOpenEditor: () => void;
  onNewChat: () => void;
  onSelectConversation: (id: string) => void;
  onDeleteConversation: (id: string) => void;
  onDeleteGroup: () => void;
}) {
  const [expanded, setExpanded] = useState(true);

  return (
    <div className="mt-0.5">
      <div
        className={cn(
          "group/grp relative flex h-9 items-center gap-1.5 overflow-hidden rounded-lg pl-2 pr-1 text-[13px]",
          isEditing
            ? "bg-card text-foreground ring-1 ring-border shadow-[0_1px_2px_rgba(0,0,0,0.05)]"
            : "text-sidebar-foreground/75 hover:bg-card/50 hover:text-sidebar-foreground",
        )}
      >
        {isEditing && (
          <span
            aria-hidden
            className="pointer-events-none absolute inset-0 rounded-lg opacity-[0.045]"
            style={{
              backgroundImage:
                "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
            }}
          />
        )}

        <button
          type="button"
          onClick={onOpenEditor}
          className="relative z-10 flex min-w-0 flex-1 items-center gap-2"
        >
          <IconFolder
            size={15}
            className={cn(
              "shrink-0",
              isEditing ? "text-foreground/80" : "text-muted-foreground/65",
            )}
          />
          <span
            className={cn("min-w-0 truncate font-medium", isEditing && "font-semibold")}
          >
            {group.name}
          </span>
          {overview && (
            <span className="shrink-0" title={`Overview: ${overview.status}`}>
              {(overview.status === "queued" || overview.status === "running") && (
                <span className="pulse-soft inline-block h-1.5 w-1.5 rounded-full bg-amber-400" />
              )}
              {overview.status === "completed" && (
                <span className="inline-block h-1.5 w-1.5 rounded-full bg-emerald-400" />
              )}
              {overview.status === "failed" && (
                <span className="inline-block h-1.5 w-1.5 rounded-full bg-red-400/70" />
              )}
            </span>
          )}
        </button>

        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            setExpanded((v) => !v);
          }}
          aria-label={expanded ? "Collapse" : "Expand"}
          className="relative z-10 grid h-6 w-6 shrink-0 place-items-center rounded text-muted-foreground/55 hover:bg-foreground/[0.06] hover:text-foreground"
        >
          <IconChevronDown
            size={12}
            className={cn("transition-transform", !expanded && "-rotate-90")}
          />
        </button>

        <div className="relative z-10 flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity group-hover/grp:opacity-100">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onNewChat();
            }}
            aria-label="New chat in this company"
            className="grid h-6 w-6 place-items-center rounded text-muted-foreground/60 hover:bg-foreground/[0.08] hover:text-foreground"
          >
            <IconPlus size={11} />
          </button>
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onDeleteGroup();
            }}
            aria-label="Delete company"
            className="grid h-6 w-6 place-items-center rounded text-muted-foreground/60 hover:bg-red-500/10 hover:text-red-400"
          >
            <IconTrash size={11} />
          </button>
        </div>
      </div>

      {expanded && (
        <div className="ml-[18px] border-l border-foreground/20 pl-4">
          {conversations.length === 0 ? (
            <button
              type="button"
              onClick={onNewChat}
              className="flex h-8 items-center gap-2 rounded-md px-2 text-[12px] text-muted-foreground/45 hover:text-muted-foreground"
            >
              <IconPlus size={10} />
              New chat
            </button>
          ) : (
            conversations.map((c) => (
              <ConversationRow
                key={c.id}
                conversation={c}
                isActive={c.id === activeId}
                onClick={() => onSelectConversation(c.id)}
                onDelete={() => onDeleteConversation(c.id)}
              />
            ))
          )}
        </div>
      )}
    </div>
  );
}

function UngroupedSection({
  conversations,
  activeId,
  onSelectConversation,
  onDeleteConversation,
  onNewChat,
}: {
  conversations: Conversation[];
  activeId: string | null;
  onSelectConversation: (id: string) => void;
  onDeleteConversation: (id: string) => void;
  onNewChat: () => void;
}) {
  const [expanded, setExpanded] = useState(true);

  return (
    <div className="mt-1">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-[13px] text-sidebar-foreground/65 hover:bg-card/50 hover:text-sidebar-foreground"
      >
        <IconChat size={14} className="shrink-0 text-muted-foreground/55" />
        <span className="font-medium">Conversations</span>
        <span className="ml-1 inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-foreground/[0.07] px-1.5 text-[10.5px] font-semibold tabular-nums text-muted-foreground/70">
          {conversations.length}
        </span>
        <IconChevronDown
          size={12}
          className={cn(
            "ml-auto shrink-0 text-muted-foreground/55 transition-transform",
            !expanded && "-rotate-90",
          )}
        />
      </button>
      {expanded && (
        <div className="ml-[18px] border-l border-foreground/20 pl-4">
          {conversations.map((c) => (
            <ConversationRow
              key={c.id}
              conversation={c}
              isActive={c.id === activeId}
              onClick={() => onSelectConversation(c.id)}
              onDelete={() => onDeleteConversation(c.id)}
            />
          ))}
          <button
            type="button"
            onClick={onNewChat}
            className="flex h-8 items-center gap-2 rounded-md px-2 text-[12px] text-muted-foreground/40 hover:text-muted-foreground"
          >
            <IconPlus size={10} />
            New chat
          </button>
        </div>
      )}
    </div>
  );
}

function ConversationRow({
  conversation,
  isActive,
  onClick,
  onDelete,
}: {
  conversation: Conversation;
  isActive: boolean;
  onClick: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      className={cn(
        "group relative flex h-9 items-center gap-1 rounded-md pr-1 text-[13px] transition-colors",
        isActive
          ? "text-foreground"
          : "text-sidebar-foreground/55 hover:text-sidebar-foreground",
      )}
    >
      <span
        aria-hidden
        className={cn(
          "pointer-events-none absolute top-1/2 -left-[16px] h-[2px] w-[6px] -translate-y-1/2 rounded-full transition-colors",
          isActive
            ? "bg-foreground"
            : "bg-muted-foreground/40 group-hover:bg-foreground/70",
        )}
      />
      <button
        type="button"
        onClick={onClick}
        className="relative min-w-0 flex-1 truncate px-2 py-1.5 text-left"
        title={conversation.title}
      >
        {conversation.title}
      </button>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
        aria-label="Delete chat"
        className={cn(
          "relative grid h-6 w-6 shrink-0 place-items-center rounded text-muted-foreground/60 opacity-0 transition",
          "hover:bg-red-500/10 hover:text-red-400 group-hover:opacity-100 focus:opacity-100",
        )}
      >
        <IconTrash size={12} />
      </button>
    </div>
  );
}
