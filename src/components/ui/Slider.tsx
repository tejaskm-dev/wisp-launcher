import { useId, type CSSProperties, type ReactNode } from "react";

interface SliderProps {
  label: string;
  description?: ReactNode;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
  format: (value: number) => string;
}

/** A labelled range input with its value shown on the right. */
export function Slider({ label, description, value, min, max, step, onChange, format }: SliderProps) {
  const id = useId();
  const fill = ((value - min) / (max - min)) * 100;

  return (
    <div className="py-3.5">
      <div className="flex items-baseline justify-between gap-4">
        <label htmlFor={id} className="text-[15px] font-semibold">
          {label}
        </label>
        <output htmlFor={id} className="font-display text-lg font-bold tabular-nums">
          {format(value)}
        </output>
      </div>
      {description && <p className="mt-0.5 text-[13px] leading-relaxed text-mute">{description}</p>}
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        aria-valuetext={format(value)}
        onChange={(event) => onChange(Number(event.target.value))}
        style={{ "--fill": `${fill}%` } as CSSProperties}
        className="wisp-range mt-4 w-full"
      />
      <div aria-hidden="true" className="mt-2 flex justify-between text-xs font-semibold text-mute-2">
        <span>{format(min)}</span>
        <span>{format(max)}</span>
      </div>
    </div>
  );
}
