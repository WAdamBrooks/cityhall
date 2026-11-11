import type { MeetingParticipant } from "./api";

export interface PersonGroup {
  person: MeetingParticipant;
  /// That person's agents in the room, other than permanent residents.
  agents: MeetingParticipant[];
}

export interface RoomGroups {
  /// Permanent resident agents, whoever owns them.
  permanent: MeetingParticipant[];
  /// Each person with their own agents beneath them.
  people: PersonGroup[];
  /// Agents whose owner is no longer in the room.
  strays: MeetingParticipant[];
}

/// Splits the room for display: permanent agents first, then each person with
/// their agents, in the order they came in.
export function roomGroups(participants: MeetingParticipant[]): RoomGroups {
  const permanent = participants.filter((p) => p.kind === "agent" && p.permanent);
  const others = participants.filter((p) => p.kind === "agent" && !p.permanent);
  const people = participants
    .filter((p) => p.kind === "person")
    .map((person) => ({ person, agents: others.filter((a) => a.owner_user_id === person.user_id) }));
  const placed = new Set(people.flatMap((g) => g.agents.map((a) => a.id)));
  return { permanent, people, strays: others.filter((a) => !placed.has(a.id)) };
}
