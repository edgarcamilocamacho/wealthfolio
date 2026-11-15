import { describe, expect, it } from "vitest";
import { mergeHistoryWithProjection } from "./history-chart-projection";

describe("mergeHistoryWithProjection", () => {
  const history = [
    { date: "2026-11-14", totalValue: 100, netContribution: 90, currency: "USD" },
    { date: "2026-11-15", totalValue: 101, netContribution: 90, currency: "USD" },
  ];

  it("returns history untouched without a projection", () => {
    expect(mergeHistoryWithProjection(history, undefined, "USD")).toBe(history);
  });

  it("adds projected values to shared days and appends future days", () => {
    const rows = mergeHistoryWithProjection(
      history,
      [
        { date: "2026-11-13", value: 95 },
        { date: "2026-11-15", value: 99 },
        { date: "2026-11-16", value: 99.5 },
      ],
      "EUR",
    );
    expect(rows.map((row) => row.date)).toEqual(["2026-11-14", "2026-11-15", "2026-11-16"]);
    expect(rows[1]).toMatchObject({ totalValue: 101, projectedValue: 99 });
    expect(rows[2]).toEqual({ date: "2026-11-16", currency: "USD", projectedValue: 99.5 });
  });

  it("keeps a projection with no history", () => {
    const rows = mergeHistoryWithProjection([], [{ date: "2026-11-16", value: 5 }], "EUR");
    expect(rows).toEqual([{ date: "2026-11-16", currency: "EUR", projectedValue: 5 }]);
  });
});
