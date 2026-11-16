import { cn } from "../../lib/cn";
import { SpriteIcon } from "./InstanceAvatar";
import { INSTANCE_ICONS, builtinIcon, iconId } from "./icons";

/** A grid of Wisp's built-in icons; arrow keys and Tab move through it like any radio group. */
export function IconPicker({ value, onChange }: { value: string | null; onChange: (icon: string) => void }) {
  const selected = builtinIcon(value);
  return (
    <div role="radiogroup" aria-label="Icon" className="grid grid-cols-10 gap-1.5">
      {INSTANCE_ICONS.map((name, index) => {
        const checked = index === selected;
        return (
          <button
            key={name}
            type="button"
            role="radio"
            aria-checked={checked}
            aria-label={name}
            title={name}
            onClick={() => onChange(iconId(index))}
            className={cn(
              "rounded-[10px] p-0.5 ring-2 transition-[box-shadow,transform] duration-150",
              checked ? "ring-cream" : "ring-transparent hover:scale-105 hover:ring-cream/30",
            )}
          >
            <SpriteIcon index={index} className="aspect-square w-full rounded-lg" />
          </button>
        );
      })}
    </div>
  );
}
