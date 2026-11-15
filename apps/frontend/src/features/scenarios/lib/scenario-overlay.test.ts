import { describe, expect, it } from "vitest";
import { overlayDeviation, overlayEndDate } from "./scenario-overlay";

describe("overlayEndDate", () => {
  it("caps the window at the scenario's end", () => {
    expect(overlayEndDate("2026-11-15", "2031-06-30", "1Y")).toBe("2027-11-15");
    expect(overlayEndDate("2026-11-15", "2028-01-01", "3Y")).toBe("2028-01-01");
    expect(overlayEndDate("2026-11-15", "2031-06-30", "END")).toBe("2031-06-30");
  });
});

describe("overlayDeviation", () => {
  it("compares the latest shared day", () => {
    const history = [
      { date: "2026-11-14", totalValue: 100 },
      { date: "2026-11-15", totalValue: 110 },
    ];
    expect(
      overlayDeviation(history, [
        { date: "2026-11-14", value: 100 },
        { date: "2026-11-15", value: 100 },
        { date: "2026-11-16", value: 101 },
      ]),
    ).toBeCloseTo(0.1);
    expect(overlayDeviation(history, [{ date: "2026-11-20", value: 1 }])).toBeNull();
    expect(overlayDeviation(history, undefined)).toBeNull();
  });
});
