import { useEffect, useMemo, useRef, useState } from "react";
import { useAtomValue } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import {
  loadMessagesIntoStore,
  messagesForSessionAtom,
  type OpenCodeMessage,
} from "@/stores/opencode";
import { MessageBubble } from "./MessageBubble";

interface Props {
  sessionId: string;
}

export function OpenCodeChat({ sessionId }: Props) {
  const messagesAtom = useMemo(() => messagesForSessionAtom(sessionId), [sessionId]);
  const messages = useAtomValue(messagesAtom);
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Load history once per session change. The SSE pump keeps it warm after.
  useEffect(() => {
    let cancelled = false;
    setError(null);
    invoke<unknown>("opencode_list_messages", { sessionId })
      .then((raw) => {
        if (cancelled) return;
        const list = Array.isArray(raw) ? (raw as OpenCodeMessage[]) : [];
        loadMessagesIntoStore(sessionId, list);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        const msg = e instanceof Error ? e.message : String(e);
        // SessionLocked is normal until the SSE pump finishes booting; don't
        // surface it as a user-visible error.
        if (!msg.toLowerCase().includes("session is not in a state")) {
          setError(msg);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [sessionId]);

  // Stick to bottom on new messages.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [messages.length]);

  async function send() {
    const text = draft.trim();
    if (!text || sending) return;
    setSending(true);
    setError(null);
    try {
      await invoke("opencode_send_prompt", { sessionId, text });
      setDraft("");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
    } finally {
      setSending(false);
    }
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    // Cmd/Ctrl+Enter sends; plain Enter inserts a newline.
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void send();
    }
  }

  return (
    <div className="flex h-full flex-col bg-[#0a0a0b] text-zinc-200">
      <div className="border-b border-zinc-800 px-3 py-1.5 text-xs text-zinc-500">
        OpenCode chat
      </div>

      <div ref={scrollRef} className="flex-1 overflow-auto px-3 py-2">
        {messages.length === 0 ? (
          <div className="flex h-full items-center justify-center text-sm text-zinc-600">
            no messages yet — send a prompt to start
          </div>
        ) : (
          messages.map((m) => <MessageBubble key={m.id} message={m} />)
        )}
      </div>

      {error ? (
        <div className="border-t border-red-900/40 bg-red-950/20 px-3 py-1 text-xs text-red-400">
          {error}
        </div>
      ) : null}

      <div className="border-t border-zinc-800 p-2">
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="message OpenCode (Cmd/Ctrl+Enter to send)"
          rows={3}
          className="w-full resize-none rounded border border-zinc-800 bg-zinc-950 px-2 py-1.5 text-sm text-zinc-200 outline-none placeholder:text-zinc-600 focus:border-zinc-700"
        />
        <div className="mt-1 flex justify-end">
          <button
            type="button"
            onClick={() => void send()}
            disabled={!draft.trim() || sending}
            className="rounded bg-zinc-800 px-3 py-1 text-xs text-zinc-200 hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {sending ? "sending…" : "send"}
          </button>
        </div>
      </div>
    </div>
  );
}
