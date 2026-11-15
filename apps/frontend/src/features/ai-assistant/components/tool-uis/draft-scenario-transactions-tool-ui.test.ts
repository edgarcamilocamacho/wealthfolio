import { describe, expect, it } from "vitest";
import { parseDraftScenarioResult } from "./draft-scenario-transactions-tool-ui";

describe("parseDraftScenarioResult", () => {
  const draft = {
    scenarioId: "s1",
    scenarioName: "Plan",
    anchorDate: "2026-06-30",
    endDate: "2031-06-30",
    totalRows: 60,
    request: { series: [] },
  };

  it("reads an object or a JSON string and fills missing lists", () => {
    expect(parseDraftScenarioResult(draft)).toMatchObject({
      scenarioId: "s1",
      series: [],
      transactions: [],
      accounts: [],
    });
    expect(parseDraftScenarioResult(JSON.stringify(draft))?.totalRows).toBe(60);
  });

  it("rejects results that are not drafts", () => {
    expect(parseDraftScenarioResult(null)).toBeNull();
    expect(parseDraftScenarioResult("not json")).toBeNull();
    expect(parseDraftScenarioResult({ scenarioId: "s1" })).toBeNull();
  });
});
