import type { JoinRequest } from "./api";

/// One person's open lines: themselves, and the agents they want to bring or were
/// asked to bring.
export interface RequestGroup {
  userId: number;
  username: string;
  /// The person asking or invited; null when they are already in the meeting.
  person: JoinRequest | null;
  agents: JoinRequest[];
  inRoom: boolean;
  dismissedEarlier: boolean;
}

/// Lines that are still open: an invitation not yet taken up, or a request
/// waiting for the moderator.
export function isOpen(item: JoinRequest): boolean {
  return item.status === "invited" || item.status === "pending";
}

/// The moderator's waiting room: open lines grouped by person, in the order people
/// first appeared.
export function groupRequests(items: JoinRequest[]): RequestGroup[] {
  const groups = new Map<number, RequestGroup>();
  for (const item of items) {
    if (!isOpen(item)) continue;
    let group = groups.get(item.user_id);
    if (!group) {
      group = {
        userId: item.user_id,
        username: item.username,
        person: null,
        agents: [],
        inRoom: item.requester_in_room,
        dismissedEarlier: item.dismissed_earlier,
      };
      groups.set(item.user_id, group);
    }
    if (item.agent_id === null) group.person = item;
    else group.agents.push(item);
  }
  return [...groups.values()];
}

/// How a line stands, saying who invited when someone did. "You" when the viewer
/// sent the invitation. A person invited by someone else reads "invited" until
/// they are present, then "waiting".
export function lineStatus(item: JoinRequest): string {
  const by = item.invited_by_you ? "you" : item.invited_by;
  if (item.kind === "invite" && by) {
    if (item.display === "invited") return `invited by ${by}`;
    if (item.display === "waiting") return `waiting (invited by ${by})`;
    if (item.display === "in the room") return `in the room (invited by ${by})`;
  }
  if (item.dismissed_earlier && item.display === "waiting") return "waiting (dismissed earlier)";
  return item.display === "waiting" ? "waiting for the moderator" : item.display;
}

/// A line's own name: the person's, or the agent's.
export function requestLabel(item: JoinRequest): string {
  return item.agent_name ?? item.username;
}

/// Whether a line is an invitation to an agent that its owner has yet to answer.
export function needsOwnersAnswer(item: JoinRequest): boolean {
  return item.kind === "invite" && item.status === "invited" && item.agent_id !== null;
}
