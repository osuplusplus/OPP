import * as Switch from "@radix-ui/react-switch";
import { InfoTip } from "../../../shared/components/ui";

export function Toggle({
  label,
  description,
  checked,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-xl border border-white/[.1] bg-white/[.035] p-4">
      <div className="flex items-center gap-2">
        <p className="font-semibold text-slate-100">{label}</p>
        <InfoTip text={description} />
      </div>
      <Switch.Root
        aria-label={label}
        checked={checked}
        className="relative h-6 w-11 shrink-0 rounded-full bg-slate-500 data-[state=checked]:bg-[var(--theme-primary)]"
        onCheckedChange={onChange}
      >
        <Switch.Thumb className="block size-5 translate-x-0.5 rounded-full bg-white transition-transform data-[state=checked]:translate-x-5" />
      </Switch.Root>
    </div>
  );
}
