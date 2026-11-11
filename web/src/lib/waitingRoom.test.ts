import { describe, expect, it } from "vitest";
import type { JoinRequest } from "./api";
import { groupRequests, lineStatus, needsOwnersAnswer, requestLabel } from "./waitingRoom";

function request(over: Partial<JoinRequest>): JoinRequest {
  return {
    id: 1,
    kind: "ask",
    user_id: 2,
    username: "sam",
    agent_id: null,
    agent_name: null,
    turn_mode: null,
    status: "pending",
    display: "waiting",
    invited_by_user_id: null,
    invited_by: null,
    invited_by_you: false,
    dismissed_earlier: false,
    requester_in_room: false,
    present: false,
    created_at: "2026-11-09T00:00:00Z",
    ...over,
  };
}

describe("groupRequests", () => {
  it("lists each person with the agents they want to bring", () => {
    const groups = groupRequests([
      request({ id: 1 }),
      request({ id: 2, agent_id: 10, agent_name: "Rev" }),
      request({ id: 3, user_id: 3, username: "kim" }),
      request({ id: 4, agent_id: 11, agent_name: "Scribe" }),
    ]);
    expect(groups.map((g) => g.username)).toEqual(["sam", "kim"]);
    expect(groups[0].person?.id).toBe(1);
    expect(groups[0].agents.map((a) => a.agent_name)).toEqual(["Rev", "Scribe"]);
    expect(groups[1].agents).toEqual([]);
  });

  it("has no person line for someone already in the room who only brings agents", () => {
    const [group] = groupRequests([request({ id: 5, agent_id: 10, agent_name: "Rev", requester_in_room: true })]);
    expect(group.person).toBeNull();
    expect(group.inRoom).toBe(true);
  });

  it("keeps invitations as well as requests, and drops what is decided or removed", () => {
    const groups = groupRequests([
      request({ id: 1, kind: "invite", status: "invited", display: "invited" }),
      request({ id: 2, user_id: 3, status: "approved", display: "in the room" }),
      request({ id: 3, user_id: 4, status: "denied", display: "denied" }),
      request({ id: 4, user_id: 5, status: "withdrawn", display: "withdrawn" }),
      request({ id: 5, user_id: 6, status: "declined", display: "declined" }),
    ]);
    expect(groups.map((g) => g.userId)).toEqual([2]);
  });

  it("remembers that someone was dismissed earlier", () => {
    expect(groupRequests([request({ dismissed_earlier: true })])[0].dismissedEarlier).toBe(true);
  });
});

describe("lineStatus", () => {
  const invite = { kind: "invite" as const, status: "invited" as const, invited_by: "kim" };

  it("says who invited", () => {
    expect(lineStatus(request({ ...invite, display: "invited" }))).toBe("invited by kim");
    expect(lineStatus(request({ ...invite, status: "pending", display: "waiting" }))).toBe("waiting (invited by kim)");
  });

  it("says you when the viewer sent the invitation", () => {
    expect(lineStatus(request({ ...invite, display: "invited", invited_by_you: true }))).toBe("invited by you");
  });

  it("reads a person's own request as waiting for the moderator, and notes a dismissal", () => {
    expect(lineStatus(request({}))).toBe("waiting for the moderator");
    expect(lineStatus(request({ dismissed_earlier: true }))).toBe("waiting (dismissed earlier)");
  });

  it("reads a decided line as its verdict", () => {
    expect(lineStatus(request({ status: "denied", display: "denied" }))).toBe("denied");
    expect(lineStatus(request({ status: "declined", display: "declined" }))).toBe("declined");
  });
});

describe("other helpers", () => {
  it("labels an agent line by the agent and a person line by the person", () => {
    expect(requestLabel(request({ agent_id: 1, agent_name: "Rev" }))).toBe("Rev");
    expect(requestLabel(request({}))).toBe("sam");
  });

  it("knows when an owner still has to answer an invitation to their agent", () => {
    const base = { kind: "invite" as const, status: "invited" as const, agent_id: 3 };
    expect(needsOwnersAnswer(request(base))).toBe(true);
    expect(needsOwnersAnswer(request({ ...base, status: "pending" }))).toBe(false);
    expect(needsOwnersAnswer(request({ ...base, agent_id: null }))).toBe(false);
  });
});
