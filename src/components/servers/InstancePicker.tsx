import type { Instance } from "../../data/types";
import { instanceLabel } from "../../lib/format";
import { Select } from "../ui/Select";

/** Which instance to use: most recently played first. */
export function InstancePicker({ instances, value, onChange, label = "Play with" }: { instances: Instance[]; value: string; onChange: (id: string) => void; label?: string }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[13px] font-bold text-cream-300">{label}</span>
      <Select label={label} value={value} onChange={onChange} options={instances.map((instance) => ({ value: instance.id, label: instance.name, note: instanceLabel(instance) }))} className="w-full" />
    </div>
  );
}
