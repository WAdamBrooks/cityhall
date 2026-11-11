import { describe, expect, it } from "vitest";
import { dashPattern } from "./dashPattern";

describe("dashPattern", () => {
  it("gives the same pattern for the same agent every time", () => {
    expect(dashPattern(7)).toBe(dashPattern(7));
  });

  it("gives different agents different patterns", () => {
    const patterns = new Set([1, 2, 3, 4, 5, 6, 7, 8].map(dashPattern));
    expect(patterns.size).toBeGreaterThan(5);
  });

  it("keeps dashes long and gaps short, so a thin border stays calm", () => {
    for (const seed of [1, 2, 3, 99, 12345]) {
      const lengths = dashPattern(seed).split(" ").map(Number);
      expect(lengths).toHaveLength(8);
      lengths.forEach((n, i) => {
        if (i % 2 === 0) expect(n).toBeGreaterThanOrEqual(5);
        else expect(n).toBeLessThanOrEqual(5);
        expect(Number.isInteger(n)).toBe(true);
      });
    }
  });
});
