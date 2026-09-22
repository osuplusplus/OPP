import { Check, LoaderCircle, X } from "lucide-react";
import { poolImportSession, usePoolImport } from "./importSession";
import { poolTitle } from "./model";

const steps = ["读取图池", "补全资料", "保存到 OPP", "完成"];
const phases = ["fetching", "enriching", "saving", "completed"];

export function PoolImportStatus() {
  const state = usePoolImport();
  if (!state || state.phase === "completed") return null;
  const failed = state.phase === "failed";
  const index = phases.indexOf(state.phase);
  const queued = state.phase === "queued";
  return <section className="collection-pool-import" aria-label="图池导入" aria-busy={!failed}>
    <div className="collection-pool-import-heading">
      {!failed ? <LoaderCircle size={22} className="collection-import-spinner" /> : null}
      <div><strong>{failed ? "图池导入失败" : queued ? "等待当前图池导入完成" : `正在导入 ${poolTitle(state.link.reference)}`}</strong>
        <p>图池、选图信息和你的练习记录都在收藏页。</p></div>
      {failed && <button aria-label="关闭导入提示" onClick={poolImportSession.dismiss}><X size={18} /></button>}
    </div>
    {!state.existing && <ol aria-label="导入步骤">{steps.map((step, i) => <li key={step} data-active={i === index} data-complete={i < index}><span>{i < index ? <Check size={13} /> : i + 1}</span>{step}</li>)}</ol>}
    <p role={failed ? "alert" : "status"}>{failed ? state.error : queued ? "新的图池已排队" : steps[index]}</p>
    {failed && <button className="collection-text-button" onClick={poolImportSession.retry}>重试导入</button>}
  </section>;
}
