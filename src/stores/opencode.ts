import { atom } from "jotai";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { appStore } from "./jotaiStore";

/// Subset of the OpenCode AssistantMessage / UserMessage we render. Carries
/// only the fields the chat panel needs so the wire shape can drift without
/// breaking us.
export interface OpenCodeMessage {
  id: string;
  sessionID: string;
  role: "user" | "assistant" | string;
  modelID?: string;
  providerID?: string;
  cost?: number;
  tokens?: {
    input?: number;
    output?: number;
    reasoning?: number;
    cache?: { read?: number; write?: number };
    total?: number;
  };
  time?: { created?: number; completed?: number };
  parts?: OpenCodeMessagePart[];
}

/// Message-part kinds we know how to render. `tool` and `tool_result` are
/// rendered as cards; `text` is rendered as a markdown bubble. Unknown kinds
/// are ignored.
export interface OpenCodeMessagePart {
  id?: string;
  messageID?: string;
  type: string;
  text?: string;
  tool?: { name?: string; input?: unknown };
  result?: unknown;
  state?: string;
}

/// Chat state per cluihud session id. Messages are keyed by their stable
/// `id` so subsequent `message.updated` events overwrite prior versions.
export const chatMessagesAtom = atom<Record<string, Record<string, OpenCodeMessage>>>(
  {},
);

/// Convenience derived atom returning an ordered message list for a session.
/// Sorted by `time.created` ascending, falling back to insertion order for
/// missing timestamps.
export function messagesForSessionAtom(sessionId: string) {
  return atom((get) => {
    const all = get(chatMessagesAtom);
    const bucket = all[sessionId];
    if (!bucket) return [] as OpenCodeMessage[];
    return Object.values(bucket).sort((a, b) => {
      const ta = a.time?.created ?? 0;
      const tb = b.time?.created ?? 0;
      return ta - tb;
    });
  });
}

interface MessageUpdatedPayload {
  session_id: string;
  properties: { info?: OpenCodeMessage };
}

interface MessagePartUpdatedPayload {
  session_id: string;
  properties: { part?: OpenCodeMessagePart };
}

let detachers: UnlistenFn[] = [];

/// Subscribe to OpenCode chat Tauri events. Idempotent — replaces any prior
/// subscriptions. Returns the latest detacher so tests can clean up.
export async function setupOpenCodeChatListeners(): Promise<UnlistenFn[]> {
  for (const d of detachers) d();
  detachers = [];

  const messageDetach = await listen<MessageUpdatedPayload>(
    "opencode:message-updated",
    (event) => {
      const info = event.payload.properties.info;
      if (!info?.id) return;
      const sid = event.payload.session_id;
      const all = appStore.get(chatMessagesAtom);
      const bucket = { ...(all[sid] ?? {}) };
      bucket[info.id] = { ...(bucket[info.id] ?? {}), ...info };
      appStore.set(chatMessagesAtom, { ...all, [sid]: bucket });
    },
  );

  const partDetach = await listen<MessagePartUpdatedPayload>(
    "opencode:message-part-updated",
    (event) => {
      const part = event.payload.properties.part;
      if (!part?.messageID) return;
      const sid = event.payload.session_id;
      const all = appStore.get(chatMessagesAtom);
      const bucket = { ...(all[sid] ?? {}) };
      const msg = bucket[part.messageID];
      if (!msg) {
        // Part arrived before the parent message — stash a placeholder so
        // the eventual `message.updated` merges in. Using sessionID = "" is
        // safe; the merge updates it.
        bucket[part.messageID] = {
          id: part.messageID,
          sessionID: "",
          role: "assistant",
          parts: [part],
        };
      } else {
        const existingParts = msg.parts ?? [];
        const idx = existingParts.findIndex((p) => p.id === part.id && part.id !== undefined);
        const nextParts =
          idx >= 0
            ? existingParts.map((p, i) => (i === idx ? { ...p, ...part } : p))
            : [...existingParts, part];
        bucket[part.messageID] = { ...msg, parts: nextParts };
      }
      appStore.set(chatMessagesAtom, { ...all, [sid]: bucket });
    },
  );

  detachers = [messageDetach, partDetach];
  return detachers;
}

/// Replace the message bucket for a session with the given list. Used after
/// `opencode_list_messages` returns the historical batch.
export function loadMessagesIntoStore(sessionId: string, messages: OpenCodeMessage[]) {
  const all = appStore.get(chatMessagesAtom);
  const bucket: Record<string, OpenCodeMessage> = {};
  for (const m of messages) {
    if (m.id) bucket[m.id] = m;
  }
  appStore.set(chatMessagesAtom, { ...all, [sessionId]: bucket });
}
