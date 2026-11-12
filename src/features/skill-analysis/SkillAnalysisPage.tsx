import { useState } from "react";
import { Brain, RefreshCw, Sparkles } from "lucide-react";

import { useMode } from "../../app/ModeContext";
import { ErrorPanel } from "../../shared/components/ErrorPanel";
import { PageHeader } from "../../shared/components/PageHeader";
import { Button, Card, EmptyState } from "../../shared/components/ui";
import { useSkillAnalysis } from "./api";
import { AnalysisLoadingPanel, ContributionList, PlayerOverviewStrip, SkillRadarHero, TrainingFocusStrip } from "./components";
import { filterContributions, type SkillFocus } from "./model";

export function SkillAnalysisPage() {
  const { client } = useMode();
  const query = useSkillAnalysis(client);
  const [focus, setFocus] = useState<SkillFocus>("all");
  const [search, setSearch] = useState("");
  const result = query.data;

  if (query.isLoading) {
    return (
      <div className="space-y-5">
        <PageHeader title="技能分析" />
        <AnalysisLoadingPanel />
      </div>
    );
  }

  if (query.error && !result) {
    return (
      <div className="space-y-5">
        <PageHeader title="技能分析" />
        <ErrorPanel error={query.error} onRetry={() => void query.refetch()} />
      </div>
    );
  }

  if (!result) {
    return (
      <div className="space-y-5">
        <PageHeader title="技能分析" />
        <EmptyState
          action={<Button onClick={() => void query.refetch()}><Sparkles className="size-4" />开始分析</Button>}
          description="登录 osu! 后分析你的最佳成绩，生成八维能力模型。"
          icon={<Brain className="size-6" />}
          title="还没有技能分析结果"
        />
      </div>
    );
  }

  return (
    <div className="space-y-5">
      <PageHeader
        actions={(
          <Button loading={query.isFetching || query.isRefreshing} onClick={() => void query.refresh().catch(() => undefined)} size="sm">
            <RefreshCw className="size-4" />增量刷新
          </Button>
        )}
        title={`${result.player.username} 的技能分析`}
      />
      {result.stale || query.refreshError ? (
        <Card className="border-amber-300/20 bg-amber-300/[.05] px-4 py-2.5">
          <p className="text-xs text-amber-100/90">
            {query.refreshError ? "增量刷新失败，当前显示上一次分析结果。" : "网络不可用，当前显示缓存的分析结果。"}
          </p>
        </Card>
      ) : null}
      <PlayerOverviewStrip result={result} />
      <SkillRadarHero focus={focus} onSelectFocus={setFocus} skills={result.skills} />
      <TrainingFocusStrip focus={focus} onSelect={setFocus} skills={result.skills} />
      <ContributionList
        client={client}
        focus={focus}
        items={filterContributions(result.contributions, { focus, search })}
        onSearch={setSearch}
        onSelectFocus={setFocus}
        search={search}
      />
    </div>
  );
}
