import type { ChatProps, PagePayload } from "../types";
import { ChatApp } from "../chat/chat-app";

export function ChatPage({ payload }: { payload: PagePayload }) {
  const user = payload.user;
  if (!user) {
    if (typeof window !== "undefined") window.location.replace("/login");
    return null;
  }
  const props = payload.page.props as unknown as ChatProps;
  return <ChatApp user={user} initialConversationId={props.conversation_id ?? null} />;
}
