import { describe, expect, it } from "vitest";
import type { MeetingParticipant } from "./api";
import { conversationCandidates, currentLists, dollarsToCents, parseWakeMinutes, toggleId } from "./permanent";

function agent(id: number, permanent: boolean, may: number[] = []): MeetingParticipant {
  return {
    id,
    kind: "agent",
    name: `a${id}`,
    user_id: null,
    agent_id: id,
    owner_user_id: 1,
    turn_mode: "mention_only",
    effective_turn_mode: "mention_only",
    presence: null,
    assistant: false,
    visible_to_others: true,
    permanent,
    objective: null,
    wake_every_minutes: null,
    may_address: may.map((m) => ({ id: m, name: `a${m}` })),
  };
}

describe("conversationCandidates", () => {
  it("offers only the other permanent agents", () => {
    const room = [agent(1, true), agent(2, true), agent(3, false)];
    expect(conversationCandidates(room, 1).map((p) => p.id)).toEqual([2]);
    expect(conversationCandidates(room, 9).map((p) => p.id)).toEqual([1, 2]);
  });
});

describe("toggleId", () => {
  it("adds and removes without changing the original", () => {
    const ids = [1, 2];
    expect(toggleId(ids, 3)).toEqual([1, 2, 3]);
    expect(toggleId(ids, 1)).toEqual([2]);
    expect(ids).toEqual([1, 2]);
  });
});

describe("currentLists", () => {
  it("reads each permanent agent's present list", () => {
    expect(currentLists([agent(1, true, [2]), agent(2, true), agent(3, false, [1])])).toEqual({ 1: [2], 2: [] });
  });
});

describe("parseWakeMinutes", () => {
  it("treats blank as never, and checks the range", () => {
    expect(parseWakeMinutes("  ")).toEqual({ minutes: null });
    expect(parseWakeMinutes("30")).toEqual({ minutes: 30 });
    expect(parseWakeMinutes("4")).toHaveProperty("problem");
    expect(parseWakeMinutes("1441")).toHaveProperty("problem");
    expect(parseWakeMinutes("1.5")).toHaveProperty("problem");
    expect(parseWakeMinutes("soon")).toHaveProperty("problem");
  });
});

describe("dollarsToCents", () => {
  it("turns dollars into cents and refuses nonsense", () => {
    expect(dollarsToCents("0.25")).toBe(25);
    expect(dollarsToCents("1")).toBe(100);
    expect(dollarsToCents("-1")).toBeNull();
    expect(dollarsToCents("abc")).toBeNull();
    expect(dollarsToCents("101")).toBeNull();
  });
});
