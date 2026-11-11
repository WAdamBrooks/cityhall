import type { MeetingParticipant } from "./api";

/// Whether the viewer may make and adjust permanent agent assignments: the
/// moderator, or an assistant moderator the moderator named. Everyone else works
/// through them. The server enforces the same rule.
export function canAssignPermanent(
  participants: MeetingParticipant[],
  meUserId: number,
  moderatorUserId: number,
): boolean {
  if (meUserId === moderatorUserId) return true;
  return participants.some((p) => p.user_id === meUserId && p.assistant);
}

/// Whether to offer "Dismiss" for a participant. The moderator may dismiss
/// anyone but themselves, and an agent's owner may dismiss their own agent.
/// The server enforces the same rule; this only decides what to show.
export function canDismiss(participant: MeetingParticipant, meUserId: number, moderatorUserId: number): boolean {
  if (participant.user_id === moderatorUserId) return false;
  if (meUserId === moderatorUserId) return true;
  return participant.kind === "agent" && participant.owner_user_id === meUserId;
}
