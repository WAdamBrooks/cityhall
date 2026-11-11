import { describe, expect, it } from "vitest";
import { agentFormProblem, parseTools, suggestAgentName } from "./agentForm";

describe("parseTools", () => {
  it("splits on commas and lines, trims, and drops blanks and repeats", () => {
    expect(parseTools(" read, search\n write ,, read ")).toEqual(["read", "search", "write"]);
    expect(parseTools("  ,\n ")).toEqual([]);
  });
});

describe("agentFormProblem", () => {
  const ok = { name: "Reviewer", description: "Reviews code", tools: "read" };

  it("accepts a complete specialty", () => {
    expect(agentFormProblem(ok)).toBeNull();
  });

  it("explains what is missing, one problem at a time", () => {
    expect(agentFormProblem({ ...ok, name: " " })).toMatch(/name/);
    expect(agentFormProblem({ ...ok, name: "Code Reviewer" })).toMatch(/spaces/);
    expect(agentFormProblem({ ...ok, description: "  " })).toMatch(/specialty/);
    expect(agentFormProblem({ ...ok, tools: " , " })).toMatch(/tool/);
  });
});

describe("suggestAgentName", () => {
  it("joins the words of a title with dashes", () => {
    expect(suggestAgentName("second agent")).toBe("second-agent");
    expect(suggestAgentName("hello-test!")).toBe("hello-test");
  });

  it("falls back when the title has no usable words", () => {
    expect(suggestAgentName("???")).toBe("Agent");
  });
});
