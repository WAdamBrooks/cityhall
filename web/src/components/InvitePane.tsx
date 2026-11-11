import { useEffect, useState } from "react";
import { api, ApiError, type Directory, type JoinRequest, type MeetingParticipant } from "../lib/api";
import { toggleId } from "../lib/permanent";
import { isOpen, lineStatus, requestLabel } from "../lib/waitingRoom";
import { Button, Checkbox, ErrorText, SectionLabel } from "./ui";

/// Invite people and agents from the directory, and see how each invitation
/// stands. A moderator's invitation lets the person straight in once they arrive;
/// anyone else's puts them in the waiting room for the moderator to decide. An
/// invited agent also needs its owner's yes.
export function InvitePane({
  meetingId,
  isModerator,
  participants,
  sent,
  onChanged,
}: {
  meetingId: number;
  isModerator: boolean;
  participants: MeetingParticipant[];
  sent: JoinRequest[];
  onChanged: () => void;
}) {
  const [directory, setDirectory] = useState<Directory | null>(null);
  const [people, setPeople] = useState<number[]>([]);
  const [agents, setAgents] = useState<number[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .directory()
      .then(setDirectory)
      .catch((e) => setError(e instanceof ApiError ? e.message : "could not load the directory"));
  }, []);

  const inRoomPeople = new Set(participants.map((p) => p.user_id));
  const inRoomAgents = new Set(participants.map((p) => p.agent_id));
  const invitable = {
    people: (directory?.people ?? []).filter((p) => !inRoomPeople.has(p.id)),
    agents: (directory?.agents ?? []).filter((a) => !inRoomAgents.has(a.id)),
  };

  async function send() {
    setBusy(true);
    setError(null);
    try {
      await api.invite(meetingId, { people, agents });
      setPeople([]);
      setAgents([]);
      onChanged();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "could not send the invitations");
    } finally {
      setBusy(false);
    }
  }

  async function rescind(item: JoinRequest) {
    try {
      await api.removeRequest(meetingId, item.id);
      onChanged();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "could not take the invitation back");
    }
  }

  const open = sent.filter(isOpen);
  return (
    <div className="flex flex-col gap-4">
      <p className="text-[12.5px] text-text-hint">
        {isModerator
          ? "Anyone you invite is let straight in when they arrive. An agent needs its owner to accept first."
          : "Anyone you invite waits for the moderator to let them in. An agent needs its owner to accept first."}
      </p>
      {invitable.people.length === 0 && invitable.agents.length === 0 && directory && (
        <p className="text-sm text-text-dim">Everyone in the directory is already here.</p>
      )}
      {invitable.people.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <SectionLabel>People</SectionLabel>
          {invitable.people.map((p) => (
            <Checkbox
              key={p.id}
              checked={people.includes(p.id)}
              onChange={() => setPeople(toggleId(people, p.id))}
              label={p.username}
            />
          ))}
        </div>
      )}
      {invitable.agents.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <SectionLabel>Agents</SectionLabel>
          {invitable.agents.map((a) => (
            <Checkbox
              key={a.id}
              checked={agents.includes(a.id)}
              onChange={() => setAgents(toggleId(agents, a.id))}
              label={
                <span title={a.description}>
                  {a.name} <span className="text-text-faint">({a.owner}&apos;s)</span>
                </span>
              }
            />
          ))}
        </div>
      )}
      {error && <ErrorText>{error}</ErrorText>}
      {(invitable.people.length > 0 || invitable.agents.length > 0) && (
        <Button variant="primary" onClick={() => void send()} disabled={busy || (!people.length && !agents.length)}>
          {busy ? "Inviting..." : "Invite"}
        </Button>
      )}
      {open.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <SectionLabel>Your invitations</SectionLabel>
          {open.map((item) => (
            <div key={item.id} className="flex items-center justify-between gap-2 text-sm">
              <span className="min-w-0">
                <span className="text-text">{requestLabel(item)}</span>
                <span className="block text-[12.5px] text-text-hint">{lineStatus(item)}</span>
              </span>
              <Button variant="ghost" onClick={() => void rescind(item)}>
                Take back
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
