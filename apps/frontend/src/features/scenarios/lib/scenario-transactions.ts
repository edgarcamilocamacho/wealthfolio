import type { ScenarioTransaction } from "@/lib/types";

/** Rows created together by one recurrence rule, shown as one line. */
export interface ScenarioSeriesGroup {
  seriesId: string;
  label: string;
  rows: ScenarioTransaction[];
  firstDate: string;
  lastDate: string;
  /** The amount every row shares, or null when rows were edited apart. */
  amount: number | null;
}

export interface GroupedScenarioTransactions {
  series: ScenarioSeriesGroup[];
  singles: ScenarioTransaction[];
}

/** Splits rows into series (in order of first date) and single rows (by date). */
export function groupScenarioTransactions(
  rows: ScenarioTransaction[],
): GroupedScenarioTransactions {
  const bySeries = new Map<string, ScenarioTransaction[]>();
  const singles: ScenarioTransaction[] = [];
  for (const row of rows) {
    if (row.seriesId) {
      const members = bySeries.get(row.seriesId) ?? [];
      members.push(row);
      bySeries.set(row.seriesId, members);
    } else {
      singles.push(row);
    }
  }

  const series = [...bySeries.entries()].map(([seriesId, members]): ScenarioSeriesGroup => {
    const sorted = [...members].sort((a, b) => a.date.localeCompare(b.date));
    const firstAmount = sorted[0].amount;
    return {
      seriesId,
      label: sorted[0].seriesLabel ?? "",
      rows: sorted,
      firstDate: sorted[0].date,
      lastDate: sorted[sorted.length - 1].date,
      amount: sorted.every((row) => row.amount === firstAmount) ? firstAmount : null,
    };
  });
  series.sort((a, b) => a.firstDate.localeCompare(b.firstDate) || a.label.localeCompare(b.label));
  singles.sort((a, b) => a.date.localeCompare(b.date));
  return { series, singles };
}

/** Day of the month of a YYYY-MM-DD date. */
export function dayOfMonthOf(date: string): number {
  return Number(date.slice(8, 10));
}

/** Rows dated on or before the anchor have no effect on the projection. */
export function isBeforeAnchor(date: string, anchorDate: string): boolean {
  return date <= anchorDate;
}

export interface ScenarioChartPoint {
  /** Midnight UTC of the day, for a numeric time axis. */
  time: number;
  date: string;
  real?: number;
  projected?: number;
}

/** Merges real and projected daily values into one series keyed by date. */
export function mergeRealAndProjected(
  real: { date: string; value: number }[],
  projected: { date: string; value: number }[],
): ScenarioChartPoint[] {
  const byDate = new Map<string, ScenarioChartPoint>();
  const pointFor = (date: string) => {
    let point = byDate.get(date);
    if (!point) {
      point = { time: Date.parse(`${date}T00:00:00Z`), date };
      byDate.set(date, point);
    }
    return point;
  };
  for (const entry of real) pointFor(entry.date).real = entry.value;
  for (const entry of projected) pointFor(entry.date).projected = entry.value;
  return [...byDate.values()].sort((a, b) => a.time - b.time);
}

/** Relative gap between real and projected on the latest day that has both. */
export function latestDeviation(points: ScenarioChartPoint[]): {
  date: string;
  real: number;
  projected: number;
  ratio: number | null;
} | null {
  for (let index = points.length - 1; index >= 0; index -= 1) {
    const { date, real, projected } = points[index];
    if (real !== undefined && projected !== undefined) {
      return {
        date,
        real,
        projected,
        ratio: projected === 0 ? null : (real - projected) / Math.abs(projected),
      };
    }
  }
  return null;
}
