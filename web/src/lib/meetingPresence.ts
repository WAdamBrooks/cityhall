import type { MeetingParticipant, Presence } from "./api";
import type { StatusTone } from "../components/ui";

export interface PresenceView {
  tone: StatusTone;
  glyph: string;
  label: string;
  /** Extra detail for a tooltip: what the agent last did, or why it is down. */
  title?: string;
}

const REASONS = { approval: "an approval", answer: "an answer" } as const;

/// How an agent's presence reads next to its name. A waiting agent always says
/// who it waits on and why, and an agent with no signal reads as unknown rather
/// than as available.
export function presenceView(presence: Presence | null, participants: MeetingParticipant[]): PresenceView {
  if (!presence) return { tone: "idle", glyph: "○", label: "no signal yet" };
  const title = presence.detail ?? undefined;
  switch (presence.state) {
    case "available":
      return { tone: "running", glyph: "●", label: "available", title };
    case "thinking":
      return { tone: "waiting", glyph: "◐", label: "thinking", title };
    case "working":
      return { tone: "running", glyph: "▶", label: "working", title };
    case "waiting_on": {
      const who = participants.find((p) => p.user_id !== null && p.user_id === presence.waiting_on_user_id);
      const target = who ? who.name : "its owner";
      const reason = presence.waiting_reason ? ` for ${REASONS[presence.waiting_reason]}` : "";
      return { tone: "waiting", glyph: "◔", label: `waiting on ${target}${reason}`, title };
    }
    case "rate_limited":
      return { tone: "waiting", glyph: "⏸", label: "rate limited", title };
    case "waking":
      return { tone: "idle", glyph: "○", label: "waking up", title };
    case "unreachable":
      return { tone: "error", glyph: "✕", label: "unreachable", title };
  }
}
