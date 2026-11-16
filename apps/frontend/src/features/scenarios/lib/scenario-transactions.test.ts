import type { ScenarioTransaction } from "@/lib/types";
import { describe, expect, it } from "vitest";
import {
  dayOfMonthOf,
  groupScenarioTransactions,
  isBeforeAnchor,
  latestDeviation,
  mergeRealAndProjected,
  timeAxisTicks,
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

describe("timeAxisTicks", () => {
  const day = (date: string) => Date.parse(`${date}T00:00:00Z`);
  const iso = (ticks: number[]) => ticks.map((time) => new Date(time).toISOString().slice(0, 10));

  it("puts a 20-year axis on round years", () => {
    expect(iso(timeAxisTicks(day("2026-11-16"), day("2046-12-31")))).toEqual([
      "2030-01-01",
      "2035-01-01",
      "2040-01-01",
      "2045-01-01",
    ]);
  });

  it("uses months on a short axis and never passes the limit", () => {
    expect(iso(timeAxisTicks(day("2026-11-16"), day("2027-03-10")))).toEqual([
      "2026-12-01",
      "2027-01-01",
      "2027-02-01",
      "2027-03-01",
    ]);
    for (const end of ["2027-11-16", "2030-12-31", "2046-12-31", "2126-12-31", "2526-12-31"]) {
      expect(timeAxisTicks(day("2026-11-16"), day(end)).length).toBeLessThanOrEqual(6);
    }
  });

  it("keeps a start that falls on the first of a month", () => {
    expect(iso(timeAxisTicks(day("2027-01-01"), day("2027-03-01")))).toEqual([
      "2027-01-01",
      "2027-02-01",
      "2027-03-01",
    ]);
  });

  it("falls back to the ends when no month starts in between", () => {
    expect(timeAxisTicks(day("2026-11-16"), day("2026-11-20"))).toEqual([
      day("2026-11-16"),
      day("2026-11-20"),
    ]);
    expect(timeAxisTicks(day("2026-11-16"), day("2026-11-16"))).toEqual([day("2026-11-16")]);
  });
});
