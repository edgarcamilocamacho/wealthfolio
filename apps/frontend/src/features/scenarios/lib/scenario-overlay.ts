import type { HistoryProjectionPoint } from "@/components/history-chart-projection";
import { parseLocalDate } from "@/lib/utils";
import { addYears, format } from "date-fns";

export type { HistoryProjectionPoint };

/** How far right a chart runs while a scenario is drawn on it. */
export type ScenarioFutureWindow = "1Y" | "3Y" | "END";

export const SCENARIO_FUTURE_WINDOWS: ScenarioFutureWindow[] = ["1Y", "3Y", "END"];

/** Last projected day to draw: the window past today, capped at the scenario's end. */
export function overlayEndDate(
  todayISO: string,
  scenarioEndDate: string,
  window: ScenarioFutureWindow,
): string {
  if (window === "END") return scenarioEndDate;
  const years = window === "1Y" ? 1 : 3;
  const windowEnd = format(addYears(parseLocalDate(todayISO), years), "yyyy-MM-dd");
  return windowEnd < scenarioEndDate ? windowEnd : scenarioEndDate;
}

/**
 * Real vs projected on the latest history day that the projection also covers,
 * as a ratio of the projected value. Null without an overlap or a zero projection.
 */
export function overlayDeviation(
  history: { date: string; totalValue: number }[],
  projection: HistoryProjectionPoint[] | undefined,
): number | null {
  if (!projection || projection.length === 0) return null;
  const projected = new Map(projection.map((point) => [point.date, point.value]));
  for (let index = history.length - 1; index >= 0; index -= 1) {
    const value = projected.get(history[index].date.slice(0, 10));
    if (value !== undefined) {
      return value === 0 ? null : (history[index].totalValue - value) / Math.abs(value);
    }
  }
  return null;
}
