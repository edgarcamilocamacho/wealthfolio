// Projection overlay rows for the history chart's category axis.

export interface HistoryProjectionPoint {
  date: string;
  value: number;
}

interface HistoryRow {
  date: string;
  totalValue: number;
  netContribution: number;
  currency: string;
}

/** A history row, a projected-only future row, or both. */
export interface HistoryChartRow {
  date: string;
  currency: string;
  totalValue?: number;
  netContribution?: number;
  projectedValue?: number;
}

/**
 * Merges history rows and projected points into one array keyed by date, in
 * date order, for a category axis. Projected points before the first history
 * row are dropped so the projection never stretches the chart to the left.
 */
export function mergeHistoryWithProjection(
  history: HistoryRow[],
  projection: HistoryProjectionPoint[] | undefined,
  fallbackCurrency: string,
): HistoryChartRow[] {
  if (!projection || projection.length === 0) return history;
  const firstDate = history[0]?.date.slice(0, 10);
  const currency = history[history.length - 1]?.currency ?? fallbackCurrency;
  const rows = new Map<string, HistoryChartRow>();
  for (const row of history) rows.set(row.date.slice(0, 10), { ...row });
  for (const point of projection) {
    if (firstDate && point.date < firstDate) continue;
    const existing = rows.get(point.date);
    if (existing) existing.projectedValue = point.value;
    else rows.set(point.date, { date: point.date, currency, projectedValue: point.value });
  }
  return [...rows.values()].sort((a, b) => a.date.localeCompare(b.date));
}
