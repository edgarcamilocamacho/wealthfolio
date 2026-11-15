import type { ScenarioTransaction } from "@/lib/types";
import { describe, expect, it } from "vitest";
import {
  dayOfMonthOf,
  groupScenarioTransactions,
  isBeforeAnchor,
  latestDeviation,
  mergeRealAndProjected,
} from "./scenario-transactions";

function row(
  id: string,
  date: string,
  amount: number,
  series?: { id: string; label: string },
): ScenarioTransaction {
  return {
    id,
    scenarioId: "s",
    date,
    kind: "DEPOSIT",
    accountId: "a",
    amount,
    seriesId: series?.id ?? null,
    seriesLabel: series?.label ?? null,
    createdAt: "",
    updatedAt: "",
  };
}

describe("groupScenarioTransactions", () => {
  it("groups series rows and keeps single rows apart, sorted by date", () => {
    const salary = { id: "s1", label: "Salary" };
    const grouped = groupScenarioTransactions([
      row("3", "2026-03-25", 100, salary),
      row("x", "2026-05-01", 7),
      row("1", "2026-01-25", 100, salary),
      row("y", "2026-02-01", 9),
    ]);
    expect(grouped.series).toHaveLength(1);
    expect(grouped.series[0]).toMatchObject({
      seriesId: "s1",
      label: "Salary",
      firstDate: "2026-01-25",
      lastDate: "2026-03-25",
      amount: 100,
    });
    expect(grouped.singles.map((single) => single.id)).toEqual(["y", "x"]);
  });

  it("reports no shared amount when a series row was edited apart", () => {
    const series = { id: "s1", label: "Rent" };
    const grouped = groupScenarioTransactions([
      row("1", "2026-01-01", 100, series),
      row("2", "2026-02-01", 120, series),
    ]);
    expect(grouped.series[0].amount).toBeNull();
  });
});

describe("date helpers", () => {
  it("reads the day of the month and compares with the anchor", () => {
    expect(dayOfMonthOf("2026-02-28")).toBe(28);
    expect(isBeforeAnchor("2026-01-01", "2026-01-01")).toBe(true);
    expect(isBeforeAnchor("2026-01-02", "2026-01-01")).toBe(false);
  });
});

describe("mergeRealAndProjected", () => {
  it("joins both series by date and finds the latest overlap", () => {
    const points = mergeRealAndProjected(
      [
        { date: "2026-01-02", value: 105 },
        { date: "2026-01-01", value: 100 },
      ],
      [
        { date: "2026-01-01", value: 100 },
        { date: "2026-01-02", value: 110 },
        { date: "2026-01-03", value: 120 },
      ],
    );
    expect(points.map((point) => point.date)).toEqual(["2026-01-01", "2026-01-02", "2026-01-03"]);
    expect(points[2]).toMatchObject({ projected: 120 });
    expect(points[2].real).toBeUndefined();

    const deviation = latestDeviation(points);
    expect(deviation?.date).toBe("2026-01-02");
    expect(deviation?.ratio).toBeCloseTo(-5 / 110);
  });

  it("has no deviation without an overlap", () => {
    expect(
      latestDeviation(mergeRealAndProjected([], [{ date: "2026-01-01", value: 1 }])),
    ).toBeNull();
  });
});
