import { MessageCircle, Send, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { getChat, onChat, sendChat } from "../../data/api";
import type { ChatMessage, Conversation } from "../../data/types";
import { cn } from "../../lib/cn";
import { timeAgo } from "../../lib/format";

/** A conversation: with one friend ("friend:<code>") or a whole party ("party:<id>"). */
export function ChatPanel({
  conversation,
  title,
  me,
  nameOf,
  now,
  onClose,
  className,
  onFailed,
}: {
  conversation: Conversation;
  title: string;
  /** Your own friend code. */
  me: string;
  nameOf: (code: string) => string;
  now: number;
  onClose?: () => void;
  className?: string;
  onFailed: (message: string) => void;
}) {
  const [messages, setMessages] = useState<ChatMessage[] | null>(null);
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const list = useRef<HTMLDivElement>(null);

  const load = useCallback(() => {
    getChat(conversation).then(setMessages, () => setMessages([]));
  }, [conversation]);
  useEffect(() => {
    load();
    return onChat((arrived) => arrived === conversation && load());
  }, [conversation, load]);
  useEffect(() => {
    const element = list.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [messages]);

  const send = async () => {
    const message = text.trim();
    if (!message || sending) return;
    setSending(true);
    try {
      setMessages(await sendChat(conversation, message));
      setText("");
    } catch (failure: unknown) {
      onFailed(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setSending(false);
    }
  };

  return (
    <section aria-label={title} className={cn("flex flex-col overflow-hidden rounded-[20px] border border-cream/8 bg-ink-900/60", className)}>
      <header className="flex items-center gap-2 border-b border-cream/7 px-3.5 py-2.5">
        <MessageCircle size={15} className="text-ember-soft" aria-hidden="true" />
        <h3 className="min-w-0 flex-1 truncate text-sm font-bold">{title}</h3>
        {onClose && (
          <button type="button" aria-label="Close the chat" onClick={onClose} className="grid size-7 place-items-center rounded-full text-mute hover:bg-cream/8 hover:text-cream">
            <X size={14} aria-hidden="true" />
          </button>
        )}
      </header>
      <div ref={list} role="log" aria-live="polite" className="flex max-h-72 min-h-28 flex-col gap-2 overflow-y-auto px-3.5 py-3">
        {messages === null ? null : messages.length === 0 ? (
          <p className="m-auto text-center text-xs text-mute">No messages yet. They're end-to-end encrypted, like everything else here.</p>
        ) : (
          messages.map((message, index) => {
            const mine = message.from === me;
            const showName = !mine && (index === 0 || messages[index - 1].from !== message.from);
            return (
              <div key={message.id} className={cn("flex max-w-[85%] flex-col", mine ? "items-end self-end" : "items-start self-start")}>
                {showName && <span className="mb-0.5 px-1 text-[11px] font-bold text-mute">{nameOf(message.from)}</span>}
                <p
                  title={timeAgo(message.at, now)}
                  className={cn("whitespace-pre-wrap break-words rounded-2xl px-3 py-1.5 text-sm leading-snug", mine ? "rounded-br-md bg-cream text-ink-900" : "rounded-bl-md bg-cream/8 text-cream-200")}
                >
                  {message.text}
                </p>
              </div>
            );
          })
        )}
      </div>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
        className="flex items-center gap-2 border-t border-cream/7 p-2"
      >
        <label className="min-w-0 flex-1">
          <span className="sr-only">Message</span>
          <input value={text} maxLength={1000} onChange={(event) => setText(event.target.value)} placeholder="Say something" className="h-9 w-full rounded-xl bg-transparent px-2 text-sm text-cream outline-none placeholder:text-mute-2" />
        </label>
        <button type="submit" aria-label="Send" disabled={!text.trim() || sending} className="grid size-9 place-items-center rounded-full bg-cream text-ink-900 transition-opacity disabled:opacity-40">
          <Send size={14} aria-hidden="true" />
        </button>
      </form>
    </section>
  );
}
