import "../styles/stageBackground.css";
import { useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";

function Artwork({ source }: { source: string }) {
  const [failed, setFailed] = useState(false);
  return failed ? <div className="local-stage-art-fallback" /> : <img src={source} alt="" onError={() => setFailed(true)} />;
}

export function StageBackground({ source, reduceMotion = false }: { source: string | null; reduceMotion?: boolean }) {
  const systemReduced = useReducedMotion();
  const reduced = reduceMotion || systemReduced;
  return <div className="local-stage-art" aria-hidden="true">
    <AnimatePresence initial={false}>
      <motion.div className="local-stage-art-layer" key={source ?? "empty"} initial={{ opacity: 0, scale: reduced ? 1 : 1.025 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0 }} transition={{ duration: reduced ? 0 : 0.4 }}>
        {source ? <Artwork key={source} source={source} /> : <div className="local-stage-art-fallback" />}
      </motion.div>
    </AnimatePresence>
    <div className="local-stage-art-shade" />
  </div>;
}
