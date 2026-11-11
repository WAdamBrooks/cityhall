import { describe, expect, it } from "vitest";
import type { MeetingParticipant } from "./api";
import { roomGroups } from "./roomGroups";

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
const person = (id: number, user_id: number): MeetingParticipant => ({ ...base, id, user_id });
const agent = (id: number, owner: number, permanent = false): MeetingParticipant => ({
  ...base,
  id,
  kind: "agent",
  agent_id: id,
  owner_user_id: owner,
  permanent,
});

describe("roomGroups", () => {
  it("puts permanent agents first and each person's agents under them", () => {
    const g = roomGroups([person(1, 10), agent(2, 10), person(3, 20), agent(4, 20, true), agent(5, 20)]);
    expect(g.permanent.map((p) => p.id)).toEqual([4]);
    expect(g.people.map((x) => [x.person.id, x.agents.map((a) => a.id)])).toEqual([
      [1, [2]],
      [3, [5]],
    ]);
    expect(g.strays).toEqual([]);
  });

  it("keeps an agent whose owner has left", () => {
    expect(roomGroups([person(1, 10), agent(2, 99)]).strays.map((p) => p.id)).toEqual([2]);
  });
});
