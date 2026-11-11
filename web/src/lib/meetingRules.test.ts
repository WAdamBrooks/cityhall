import { describe, expect, it } from "vitest";
import type { MeetingParticipant } from "./api";
import { canAssignPermanent, canDismiss } from "./meetingRules";

const base: MeetingParticipant = {
  id: 1,
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
const MODERATOR = 1;
const OWNER = 2;
const OTHER = 3;

const person = (user_id: number): MeetingParticipant => ({ ...base, user_id });
const agent = (owner_user_id: number): MeetingParticipant => ({
  ...base,
  kind: "agent",
  agent_id: 5,
  owner_user_id,
});

describe("canAssignPermanent", () => {
  it("is for the moderator and the assistants the moderator named", () => {
    const room = [person(OWNER), { ...person(OTHER), assistant: true }];
    expect(canAssignPermanent(room, MODERATOR, MODERATOR)).toBe(true);
    expect(canAssignPermanent(room, OTHER, MODERATOR)).toBe(true);
    expect(canAssignPermanent(room, OWNER, MODERATOR)).toBe(false);
    expect(canAssignPermanent([], 99, MODERATOR)).toBe(false);
  });
});

describe("canDismiss", () => {
  it("lets the moderator dismiss anyone but themselves", () => {
    expect(canDismiss(person(OTHER), MODERATOR, MODERATOR)).toBe(true);
    expect(canDismiss(agent(OWNER), MODERATOR, MODERATOR)).toBe(true);
    expect(canDismiss(person(MODERATOR), MODERATOR, MODERATOR)).toBe(false);
  });

  it("lets an owner dismiss only their own agents", () => {
    expect(canDismiss(agent(OWNER), OWNER, MODERATOR)).toBe(true);
    expect(canDismiss(agent(OTHER), OWNER, MODERATOR)).toBe(false);
    expect(canDismiss(person(OTHER), OWNER, MODERATOR)).toBe(false);
  });

  it("offers nothing to someone with no standing", () => {
    expect(canDismiss(agent(OWNER), OTHER, MODERATOR)).toBe(false);
  });
});
