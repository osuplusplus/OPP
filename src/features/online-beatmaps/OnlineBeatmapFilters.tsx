import { useState, type ReactNode } from "react";
import { ChevronDown, ChevronUp, Filter, RotateCcw } from "lucide-react";
import { Badge, Button, Card, InfoTip, Input } from "../../shared/components/ui";
import type { OnlineBeatmapSearchQuery, Ruleset } from "../../shared/types/osu";
import { SearchAutocomplete, type SearchSuggestion } from "../../shared/components/SearchAutocomplete";
import { activeFilterCount, genreOptions, languageOptions, parseOptionalNumber, statusOptions } from "./filters";

const inputClass = "w-full px-3 py-2.5 text-sm placeholder:text-slate-600";

function Field({ label, children }: { label: string; children: ReactNode }) {
  return <label className="block"><span className="mb-2 block text-sm font-medium text-slate-400">{label}</span>{children}</label>;
}

function FilterRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1.5 border-t border-[var(--line-subtle)] py-2.5 first:border-t-0 sm:grid-cols-[4.75rem_minmax(0,1fr)] sm:gap-3">
      <div className="whitespace-nowrap pt-1 text-[11px] font-semibold text-slate-500">{label}</div>
      <div aria-label={`${label}筛选`} className="flex min-w-0 flex-nowrap items-center gap-1.5 overflow-x-auto overscroll-x-contain pb-0.5 [scrollbar-color:var(--line-strong)_transparent] [scrollbar-width:thin]" role="group">{children}</div>
    </div>
  );
}

function TextOption({ active, children, onClick }: { active: boolean; children: ReactNode; onClick: () => void }) {
  return (
    <button
      aria-pressed={active}
      className={`shrink-0 whitespace-nowrap rounded-md px-1.5 py-1 text-left transition-colors ${active ? "bg-[var(--theme-primary-muted)] font-semibold text-[var(--theme-primary-light)]" : "font-normal text-slate-500 hover:bg-[var(--surface-interactive)] hover:text-slate-200"}`}
      onClick={onClick}
      type="button"
    >
      {/* 文本节点独立控制字号，避免全局原生按钮字体继承规则覆盖组件排版。 */}
      <span className="text-[11px] leading-4">{children}</span>
    </button>
  );
}

function Range({ label, min, max, onMin, onMax }: { label: string; min: number | null; max: number | null; onMin: (value: number | null) => void; onMax: (value: number | null) => void }) {
  return <Field label={label}><div className="grid grid-cols-[1fr_auto_1fr] items-center gap-2"><Input className={inputClass} inputMode="decimal" onChange={(event) => onMin(parseOptionalNumber(event.target.value))} placeholder="最小" type="number" value={min ?? ""} /><span className="text-slate-600">至</span><Input className={inputClass} inputMode="decimal" onChange={(event) => onMax(parseOptionalNumber(event.target.value))} placeholder="最大" type="number" value={max ?? ""} /></div></Field>;
}

const rulesetOptions: ReadonlyArray<{ value: Ruleset | null; label: string }> = [
  { value: null, label: "全部" },
  { value: "osu", label: "osu!" },
  { value: "taiko", label: "osu!taiko" },
  { value: "fruits", label: "osu!catch" },
  { value: "mania", label: "osu!mania" },
];
const maniaKeyOptions = [4, 5, 6, 7, 8, 9, 10] as const;
const contentOptions = [
  ["", "全部"], ["recommended", "推荐难度"], ["converts", "包括转谱"], ["follows", "已关注谱师"], ["spotlights", "聚光灯谱面"], ["featured_artists", "精选艺术家"],
] as const;
const gradeOptions = [["", "全部"], ["XH", "银 SS"], ["X", "SS"], ["SH", "银 S"], ["S", "S"], ["A", "A"], ["B", "B"], ["C", "C"], ["D", "D"]] as const;
const playedOptions = [["", "全部"], ["played", "玩过"], ["unplayed", "没玩过"]] as const;

export function OnlineBeatmapFilters({ query, loading, onChange, onReset, onSubmit, suggestions = [] }: { query: OnlineBeatmapSearchQuery; loading: boolean; onChange: (query: OnlineBeatmapSearchQuery) => void; onReset: () => void; onSubmit: (query: OnlineBeatmapSearchQuery) => void; suggestions?: SearchSuggestion[] }) {
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const patch = (value: Partial<OnlineBeatmapSearchQuery>, submit = false) => {
    const sort = Object.prototype.hasOwnProperty.call(value, "query")
      ? value.query?.trim() ? "relevance_desc" : "ranked_desc"
      : value.sort ?? query.sort;
    const next = { ...query, ...value, sort, cursor_string: null };
    onChange(next);
    // 离散选项直接使用本次生成的查询，避免等待 React 状态更新后读到旧值。
    if (submit) onSubmit(next);
  };
  const select = (value: Partial<OnlineBeatmapSearchQuery>) => patch(value, true);
  const count = activeFilterCount(query);
  const toggleExtra = (extra: "video" | "storyboard") => select({ extras: query.extras.includes(extra) ? query.extras.filter((item) => item !== extra) : [...query.extras, extra] });

  return (
    <Card className="opp-beatmap-filter opp-online-panel overflow-hidden rounded-[11px] border border-[var(--line-subtle)] bg-[color-mix(in_srgb,var(--surface-panel)_94%,transparent)] shadow-[0_14px_34px_rgba(0,0,0,0.08)]" unstyled>
      <form onSubmit={(event) => { event.preventDefault(); onSubmit(query); }}>
        <div className="flex items-center gap-3 border-b border-[var(--line-subtle)] px-5 py-3.5">
          <Filter className="size-4 text-[var(--theme-primary)]" />
          <h2 className="text-sm font-semibold text-white">搜索与筛选</h2>
          {count ? <Badge tone="cyan">{count} 项条件</Badge> : null}
          <div className="ml-auto flex gap-2"><Button aria-label="重置筛选" onClick={onReset} size="icon" type="button" variant="ghost"><RotateCcw className="size-4" /></Button><Button loading={loading} size="sm" type="submit">应用筛选</Button></div>
        </div>

        <div className="px-5 pb-1">
          <div className="relative py-4" data-page-guide-online-search="true"><SearchAutocomplete ariaLabel="搜索在线谱面" className="w-full" inputClassName={`opp-input ${inputClass} py-3 pl-11 pr-10 text-[15px]`} onChange={(value) => patch({ query: value })} placeholder="搜索标题、艺术家、谱师、标签或谱面 ID" suggestions={suggestions} value={query.query} /><span className="absolute right-3 top-1/2 z-20 -translate-y-1/2"><InfoTip text="自动补全只基于当前已加载的搜索结果。输入后点击“应用筛选”进行完整的在线搜索。" /></span></div>

          {/* 首屏只保留最常用条件，其余能力继续复用原有查询字段。 */}
          <div className="border-y border-[var(--line-subtle)]" data-page-guide-online-core-filters="true">
            <FilterRow label="模式">{rulesetOptions.map((option) => <TextOption active={query.ruleset === option.value} key={option.value ?? "all"} onClick={() => select({ ruleset: option.value })}>{option.label}</TextOption>)}</FilterRow>
            <FilterRow label="状态">{statusOptions.map((option) => <TextOption active={query.status === option.value} key={option.value} onClick={() => select({ status: option.value })}>{option.value === "any" ? "全部" : option.label}</TextOption>)}</FilterRow>
          </div>

          <div data-page-guide-online-advanced="true">
            {!advancedOpen ? (
              <button aria-expanded="false" className="flex w-full items-center justify-center gap-2 py-3.5 text-slate-500 transition hover:text-white" onClick={() => setAdvancedOpen(true)} type="button">
                <ChevronDown className="size-4" />
                <span className="text-xs font-semibold">更多筛选{count ? ` · ${count}` : ""}</span>
              </button>
            ) : (
              <>
                <div className="space-y-5 pt-4">
                  <div className="border-y border-[var(--line-subtle)]">
                    <FilterRow label="流派">{genreOptions.map((option) => <TextOption active={query.genre === option.value} key={option.value ?? "all"} onClick={() => select({ genre: option.value })}>{option.value === null ? "全部" : option.label}</TextOption>)}</FilterRow>
                    <FilterRow label="语言">{languageOptions.map((option) => <TextOption active={query.language === option.value} key={option.value ?? "all"} onClick={() => select({ language: option.value })}>{option.value === null ? "全部" : option.label}</TextOption>)}</FilterRow>
                    {query.ruleset === "mania" ? <FilterRow label="键数"><TextOption active={query.keys_min === null && query.keys_max === null} onClick={() => select({ keys_min: null, keys_max: null })}>全部</TextOption>{maniaKeyOptions.map((keys) => <TextOption active={query.keys_min === keys && query.keys_max === keys} key={keys} onClick={() => select({ keys_min: keys, keys_max: keys })}>{keys}K</TextOption>)}</FilterRow> : null}
                    <FilterRow label="内容">{contentOptions.map(([value, label]) => <TextOption active={query.content_filter === value} key={value || "all"} onClick={() => select({ content_filter: value })}>{label}</TextOption>)}</FilterRow>
                    <FilterRow label="成绩">{gradeOptions.map(([value, label]) => <TextOption active={query.grade === value} key={value || "all"} onClick={() => select({ grade: value })}>{label}</TextOption>)}</FilterRow>
                    <FilterRow label="游玩状态">{playedOptions.map(([value, label]) => <TextOption active={query.played === value} key={value || "all"} onClick={() => select({ played: value })}>{label}</TextOption>)}</FilterRow>
                    <FilterRow label="附加内容"><TextOption active={!query.include_nsfw} onClick={() => select({ include_nsfw: false })}>安全内容</TextOption><TextOption active={query.include_nsfw} onClick={() => select({ include_nsfw: true })}>包括成人内容</TextOption><TextOption active={query.extras.includes("video")} onClick={() => toggleExtra("video")}>有视频</TextOption><TextOption active={query.extras.includes("storyboard")} onClick={() => toggleExtra("storyboard")}>有故事板</TextOption></FilterRow>
                  </div>
                  <div className="space-y-4 border-b border-[var(--line-subtle)] pb-5">
                    <div className="grid gap-4 md:grid-cols-2"><Field label="艺术家"><Input className={inputClass} onChange={(event) => patch({ artist: event.target.value })} value={query.artist} /></Field><Field label="标题"><Input className={inputClass} onChange={(event) => patch({ title: event.target.value })} value={query.title} /></Field><Field label="Mapper"><Input className={inputClass} onChange={(event) => patch({ mapper: event.target.value })} value={query.mapper} /></Field><Field label="来源"><Input className={inputClass} onChange={(event) => patch({ source: event.target.value })} value={query.source} /></Field><Field label="标签"><Input className={inputClass} onChange={(event) => patch({ tags: event.target.value })} placeholder="逗号分隔" value={query.tags} /></Field></div>
                    <div className="grid gap-4 md:grid-cols-2"><Field label="Rank 日期范围"><div className="grid grid-cols-2 gap-2"><Input className={inputClass} onChange={(event) => patch({ ranked_from: event.target.value })} type="date" value={query.ranked_from} /><Input className={inputClass} onChange={(event) => patch({ ranked_to: event.target.value })} type="date" value={query.ranked_to} /></div></Field><Field label="提交日期范围"><div className="grid grid-cols-2 gap-2"><Input className={inputClass} onChange={(event) => patch({ submitted_from: event.target.value })} type="date" value={query.submitted_from} /><Input className={inputClass} onChange={(event) => patch({ submitted_to: event.target.value })} type="date" value={query.submitted_to} /></div></Field></div>
                  </div>
                  <div className="grid gap-4 pb-1 md:grid-cols-2"><Range label="星数" max={query.stars_max} min={query.stars_min} onMax={(stars_max) => patch({ stars_max })} onMin={(stars_min) => patch({ stars_min })} /><Range label="BPM" max={query.bpm_max} min={query.bpm_min} onMax={(bpm_max) => patch({ bpm_max })} onMin={(bpm_min) => patch({ bpm_min })} /><Range label="长度（秒）" max={query.length_max} min={query.length_min} onMax={(length_max) => patch({ length_max })} onMin={(length_min) => patch({ length_min })} /><Range label="收藏数" max={query.favourites_max} min={query.favourites_min} onMax={(favourites_max) => patch({ favourites_max })} onMin={(favourites_min) => patch({ favourites_min })} /><Range label="AR" max={query.ar_max} min={query.ar_min} onMax={(ar_max) => patch({ ar_max })} onMin={(ar_min) => patch({ ar_min })} /><Range label="CS" max={query.cs_max} min={query.cs_min} onMax={(cs_max) => patch({ cs_max })} onMin={(cs_min) => patch({ cs_min })} /><Range label="OD" max={query.od_max} min={query.od_min} onMax={(od_max) => patch({ od_max })} onMin={(od_min) => patch({ od_min })} /><Range label="HP" max={query.hp_max} min={query.hp_min} onMax={(hp_max) => patch({ hp_max })} onMin={(hp_min) => patch({ hp_min })} /></div>
                </div>
                <button aria-expanded="true" className="mt-4 flex w-full items-center justify-center gap-2 border-t border-[var(--line-subtle)] py-3.5 text-slate-500 transition hover:text-white" onClick={() => setAdvancedOpen(false)} type="button">
                  <ChevronUp className="size-4" />
                  <span className="text-xs font-semibold">收起筛选</span>
                </button>
              </>
            )}
          </div>
        </div>
      </form>
    </Card>
  );
}
