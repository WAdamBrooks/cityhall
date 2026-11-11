import { describe, expect, it } from "vitest";
import type { MeetingParticipant, Presence } from "./api";
import { presenceView } from "./meetingPresence";

const person: MeetingParticipant = {
  id: 1,
  kind: "person",
  name: "admin",
  user_id: 7,
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

function presence(over: Partial<Presence>): Presence {
  return {
    state: "available",
    waiting_reason: null,
    waiting_on_user_id: null,
    detail: null,
    since: "2026-11-08T00:00:00Z",
    ...over,
  };
}

describe("presenceView", () => {
  it("reads a missing signal as unknown, never as available", () => {
    expect(presenceView(null, [])).toMatchObject({ tone: "idle", label: "no signal yet" });
  });

  it("shows each state with a distinct tone", () => {
    const tones = (["available", "thinking", "working", "rate_limited", "waking", "unreachable"] as const).map(
      (state) => presenceView(presence({ state }), []).tone,
    );
    expect(tones).toEqual(["running", "waiting", "running", "waiting", "idle", "error"]);
  });

  it("names who a waiting agent waits on and why", () => {
    const view = presenceView(presence({ state: "waiting_on", waiting_reason: "approval", waiting_on_user_id: 7 }), [
      person,
    ]);
    expect(view.label).toBe("waiting on admin for an approval");
  });

  it("falls back to the owner when the target is not in the meeting", () => {
    const view = presenceView(presence({ state: "waiting_on", waiting_reason: "answer", waiting_on_user_id: 99 }), [
      person,
    ]);
    expect(view.label).toBe("waiting on its owner for an answer");
  });

  it("carries the detail as a tooltip", () => {
    expect(presenceView(presence({ state: "unreachable", detail: "workspace answered 502" }), []).title).toBe(
      "workspace answered 502",
    );
  });
});
