import { describe, expect, it } from "vitest";
import type { MeetingMessage, MeetingParticipant } from "./api";
import { bubbleStyle, toneColor } from "./bubbles";

const ME = 1;
const THEM = 2;

const base: MeetingParticipant = {
  id: 0,
  kind: "person",
  name: "x",
  user_id: null,
  agent_id: null,
  owner_user_id: null,
  turn_mode: null,
  effective_turn_mode: null,
  presence: null,
  assistant: false,
  visible_to_others: true,
  permanent: false,
  objective: null,
  wake_every_minutes: null,
  may_address: [],
};

const room: MeetingParticipant[] = [
  { ...base, id: 10, user_id: ME },
  { ...base, id: 11, user_id: THEM },
  { ...base, id: 20, kind: "agent", agent_id: 1, owner_user_id: ME },
  { ...base, id: 21, kind: "agent", agent_id: 2, owner_user_id: THEM },
  { ...base, id: 22, kind: "agent", agent_id: 3, owner_user_id: THEM, permanent: true },
  { ...base, id: 23, kind: "agent", agent_id: 4, owner_user_id: ME, permanent: true },
];

function said(participant_id: number, is_agent: boolean): MeetingMessage {
  return {
    seq: 1,
    participant_id,
    speaker: "s",
    is_agent,
    dialog_type: "contribution",
    body: "hi",
    revealed: true,
    created_at: "2026-11-09T00:00:00Z",
  };
}

describe("bubbleStyle", () => {
  it("puts the reader's own words and their agents' on the right", () => {
    expect(bubbleStyle(said(10, false), room, ME)).toEqual({ side: "right", tone: "self" });
    expect(bubbleStyle(said(20, true), room, ME)).toEqual({ side: "right", tone: "agent" });
  });

  it("puts other people and their agents on the left", () => {
    expect(bubbleStyle(said(11, false), room, ME)).toEqual({ side: "left", tone: "other" });
    expect(bubbleStyle(said(21, true), room, ME)).toEqual({ side: "left", tone: "agent" });
  });

  it("centers permanent agents, whoever owns them", () => {
    expect(bubbleStyle(said(22, true), room, ME).side).toBe("center");
    expect(bubbleStyle(said(23, true), room, ME).side).toBe("center");
  });

  it("falls back to the left for someone no longer in the room", () => {
    expect(bubbleStyle(said(99, false), room, ME)).toEqual({ side: "left", tone: "other" });
  });
});

describe("toneColor", () => {
  it("names a theme variable", () => {
    expect(toneColor("agent")).toBe("var(--color-bubble-agent)");
  });
});
