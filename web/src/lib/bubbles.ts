import type { MeetingMessage, MeetingParticipant } from "./api";

/// Where a conversation box sits in the panel.
export type Side = "right" | "center" | "left";
/// Which of the three box colors it takes.
export type Tone = "self" | "agent" | "other";

export interface BubbleStyle {
  side: Side;
  tone: Tone;
}

/// How one message is drawn, from the point of view of the person reading:
///  - right: their own statements and those of their own agents;
///  - center: agents that are permanent residents of the room;
///  - left: everyone else's people and agents.
/// Color is separate from side: agents always share one color, the reader's own
/// words have another, and other people a third.
export function bubbleStyle(
  message: MeetingMessage,
  participants: MeetingParticipant[],
  viewerId: number,
): BubbleStyle {
  const speaker = participants.find((p) => p.id === message.participant_id);
  const tone: Tone = message.is_agent ? "agent" : speaker?.user_id === viewerId ? "self" : "other";
  if (message.is_agent && speaker?.permanent) return { side: "center", tone };
  const mine = message.is_agent ? speaker?.owner_user_id === viewerId : speaker?.user_id === viewerId;
  return { side: mine ? "right" : "left", tone };
}

/// The CSS color for a tone, defined once in index.css.
export function toneColor(tone: Tone): string {
  return `var(--color-bubble-${tone})`;
}
