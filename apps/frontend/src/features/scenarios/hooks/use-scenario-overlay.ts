import { usePersistentState } from "@/hooks/use-persistent-state";
import type { AccountScope } from "@/lib/types";
import { useMemo } from "react";
import {
  overlayEndDate,
  type HistoryProjectionPoint,
  type ScenarioFutureWindow,
} from "../lib/scenario-overlay";
import { useScenarioProjection, useScenarios } from "./use-scenarios";

const ACTIVE_SCENARIO_KEY = "scenario-overlay:active";
const FUTURE_WINDOW_KEY = "scenario-overlay:window";

interface Options {
  scope: AccountScope;
  /** Left edge of the chart (YYYY-MM-DD); undefined for all time. */
  startDate?: string;
  todayISO: string;
}

/**
 * The scenario drawn over history charts. The choice is a per-device UI
 * preference shared by every chart that offers it.
 */
export function useScenarioOverlay({ scope, startDate, todayISO }: Options) {
  const [activeScenarioId, setActiveScenarioId] = usePersistentState<string | null>(
    ACTIVE_SCENARIO_KEY,
    null,
  );
  const [futureWindow, setFutureWindow] = usePersistentState<ScenarioFutureWindow>(
    FUTURE_WINDOW_KEY,
    "1Y",
  );
  const { data: scenarios = [] } = useScenarios();
  const activeScenario = scenarios.find((scenario) => scenario.id === activeScenarioId) ?? null;
  const endDate = activeScenario
    ? overlayEndDate(todayISO, activeScenario.endDate, futureWindow)
    : undefined;
  const { data: projection, isLoading } = useScenarioProjection(activeScenario?.id, scope, {
    startDate,
    endDate,
  });

  const points = useMemo<HistoryProjectionPoint[] | undefined>(
    () =>
      activeScenario && projection?.scenarioId === activeScenario.id
        ? projection.points.map((point) => ({ date: point.date, value: point.value }))
        : undefined,
    [activeScenario, projection],
  );

  return {
    scenarios,
    activeScenario,
    setActiveScenarioId,
    futureWindow,
    setFutureWindow,
    points,
    warnings: projection?.warnings ?? [],
    isLoading: Boolean(activeScenario) && isLoading,
  };
}

export type ScenarioOverlayState = ReturnType<typeof useScenarioOverlay>;
