/* eslint-disable react-refresh/only-export-components */
import {
  startTransition,
  useCallback,
  createContext,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type { OsuClient, Ruleset } from "../shared/types/osu";

const RULESET_STORAGE_KEY = "opp.global-ruleset";
const CLIENT_STORAGE_KEY = "opp.global-client";
const rulesets: Ruleset[] = ["osu", "taiko", "fruits", "mania"];
const clients: OsuClient[] = ["stable", "lazer"];

function readPreference<T extends string>(key: string, values: T[]): T | null {
  if (typeof window === "undefined") return null;
  const value = window.localStorage.getItem(key);
  return values.includes(value as T) ? (value as T) : null;
}

interface ModeContextValue {
  ruleset: Ruleset;
  setRuleset: (ruleset: Ruleset) => void;
  client: OsuClient;
  setClient: (client: OsuClient) => void;
  hasRulesetPreference: boolean;
}

const ModeContext = createContext<ModeContextValue | null>(null);

export function ModeProvider({ children }: { children: ReactNode }) {
  const initialRuleset = useMemo(
    () => readPreference(RULESET_STORAGE_KEY, rulesets),
    [],
  );
  const [rulesetValue, setRulesetValue] = useState<Ruleset>(
    initialRuleset ?? "osu",
  );
  const [clientValue, setClientValue] = useState<OsuClient>(
    () => readPreference(CLIENT_STORAGE_KEY, clients) ?? "stable",
  );
  const [hasRulesetPreference, setHasRulesetPreference] = useState(
    initialRuleset !== null,
  );

  const setRuleset = useCallback((ruleset: Ruleset) => {
    startTransition(() => {
      setRulesetValue(ruleset);
      setHasRulesetPreference(true);
    });
    window.localStorage.setItem(RULESET_STORAGE_KEY, ruleset);
  }, []);

  const setClient = useCallback((client: OsuClient) => {
    startTransition(() => setClientValue(client));
    window.localStorage.setItem(CLIENT_STORAGE_KEY, client);
  }, []);

  const value = useMemo(
    () => ({
      ruleset: rulesetValue,
      setRuleset,
      client: clientValue,
      setClient,
      hasRulesetPreference,
    }),
    [
      clientValue,
      hasRulesetPreference,
      rulesetValue,
      setClient,
      setRuleset,
    ],
  );
  return <ModeContext.Provider value={value}>{children}</ModeContext.Provider>;
}

export function useMode() {
  const context = useContext(ModeContext);
  if (!context) throw new Error("useMode must be used inside ModeProvider");
  return context;
}
