import type { MotdSpan } from "../../data/types";
import { cn } from "../../lib/cn";

/** A colour from a server's message, made readable on Wisp's dark cards (black text turns grey). */
function readable(color: string | undefined): string | undefined {
  if (!color || !/^#[0-9a-f]{6}$/i.test(color)) return undefined;
  const [r, g, b] = [1, 3, 5].map((at) => parseInt(color.slice(at, at + 2), 16) / 255);
  const light = 0.2126 * r + 0.7152 * g + 0.0722 * b;
  return light < 0.16 ? "#8f897f" : color;
}

/** A server's message of the day, in its own colours and styles. */
export function Motd({ spans, className }: { spans: MotdSpan[]; className?: string }) {
  return (
    <p className={cn("whitespace-pre-line text-[13px] leading-snug text-cream-300", className)}>
      {spans.map((span, index) => (
        <span
          key={index}
          style={{ color: readable(span.color) }}
          className={cn(span.bold && "font-bold", span.italic && "italic", span.underlined && "underline", span.strikethrough && "line-through", span.obfuscated && "opacity-50")}
        >
          {span.text}
        </span>
      ))}
    </p>
  );
}
