import type { MeetingParticipant } from "./api";

/// The agents a permanent agent may be allowed to address: the other permanent
/// agents in the room.
export function conversationCandidates(participants: MeetingParticipant[], selfId: number): MeetingParticipant[] {
  return participants.filter((p) => p.kind === "agent" && p.permanent && p.id !== selfId);
}

/// A copy of `ids` with `id` added or removed.
export function toggleId(ids: number[], id: number): number[] {
  return ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id];
}

/// The lists the permanent agents already in the room have today, by agent id,
/// as the starting point for adjusting them.
export function currentLists(participants: MeetingParticipant[]): Record<number, number[]> {
  const lists: Record<number, number[]> = {};
  for (const p of participants) {
    if (p.kind === "agent" && p.permanent) lists[p.id] = p.may_address.map((a) => a.id);
  }
  return lists;
}

/// Minutes between wake-ups from what was typed. Empty means never woken. Returns
/// the minutes, or a reason the text cannot be used.
export function parseWakeMinutes(text: string): { minutes: number | null } | { problem: string } {
  const trimmed = text.trim();
  if (!trimmed) return { minutes: null };
  const minutes = Number(trimmed);
  if (!Number.isInteger(minutes) || minutes < 5 || minutes > 24 * 60) {
    return { problem: "Wake-ups must be a whole number of minutes, from 5 to 1440." };
  }
  return { minutes };
}

/// US cents from a dollar amount typed by a person, or null when it is not a
/// usable amount of at most $100.
export function dollarsToCents(text: string): number | null {
  const dollars = Number(text.trim());
  if (!Number.isFinite(dollars) || dollars < 0 || dollars > 100) return null;
  return Math.round(dollars * 100);
}
