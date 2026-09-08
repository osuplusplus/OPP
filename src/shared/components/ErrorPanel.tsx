import { AlertTriangle, RotateCw } from "lucide-react";
import { errorMessage } from "../lib/format";
import { Button, EmptyState } from "./ui";

export function ErrorPanel({
  error,
  onRetry,
}: {
  error: unknown;
  onRetry?: () => void;
}) {
  const detail = error && typeof error === "object" ? error as { code?: string; request_id?: string; message?: string } : undefined;
  const diagnostic = [
    detail?.code,
    detail?.request_id ? `请求 ${detail.request_id}` : undefined,
  ].filter(Boolean).join(" · ");
  return (
    <EmptyState
      action={
        onRetry ? (
          <Button onClick={onRetry}>
            <RotateCw className="size-4" />
            重新加载
          </Button>
        ) : undefined
      }
      description={diagnostic ? `${errorMessage(error)}（${diagnostic}）` : errorMessage(error)}
      icon={<AlertTriangle className="size-5 text-amber-200" />}
      title="数据暂时没有到达"
    />
  );
}
