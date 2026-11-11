import { useState } from "react";
import { api, ApiError, type MeetingParticipant } from "../lib/api";
import { conversationCandidates, currentLists, parseWakeMinutes, toggleId } from "../lib/permanent";
import { Button, Checkbox, ErrorText, Field, Input, Modal, SectionLabel, Textarea } from "./ui";

/// Make an agent a permanent resident, or change the terms of one. Step one is
/// the agent's own terms: whom it may address, and its objective. Step two lets
/// the moderator adjust the permanent agents already in the room, who may now
/// need to reach the new one. A permanent agent may address only agents on its
/// list, so both steps matter.
export function MakePermanentDialog({
  meetingId,
  agent,
  participants,
  onClose,
  onDone,
}: {
  meetingId: number;
  agent: MeetingParticipant;
  participants: MeetingParticipant[];
  onClose: () => void;
  onDone: () => void;
}) {
  const others = conversationCandidates(participants, agent.id);
  const [step, setStep] = useState<1 | 2>(1);
  const [mine, setMine] = useState<number[]>(agent.may_address.map((a) => a.id));
  const [objective, setObjective] = useState(agent.objective ?? "");
  const [wake, setWake] = useState(agent.wake_every_minutes?.toString() ?? "");
  const [lists, setLists] = useState<Record<number, number[]>>(currentLists(participants));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const wakeParsed = parseWakeMinutes(wake);
  const wakeProblem = "problem" in wakeParsed ? wakeParsed.problem : null;
  const wakeMinutes = "minutes" in wakeParsed ? wakeParsed.minutes : null;

  async function save() {
    setBusy(true);
    setError(null);
    try {
      await api.makePermanent(meetingId, agent.id, {
        converse_with: mine,
        others: others.map((o) => ({ participant_id: o.id, converse_with: lists[o.id] ?? [] })),
        objective: objective.trim() || undefined,
        wake_every_minutes: wakeMinutes ?? undefined,
      });
      onDone();
      onClose();
    } catch (e) {
      setError(e instanceof ApiError ? e.message : "could not save");
    } finally {
      setBusy(false);
    }
  }

  function next() {
    if (wakeProblem) {
      setError(wakeProblem);
      return;
    }
    if (wakeMinutes !== null && !objective.trim()) {
      setError("A schedule needs an objective to work on.");
      return;
    }
    setError(null);
    if (others.length === 0) void save();
    else setStep(2);
  }

  const title = agent.permanent ? `Permanent terms for ${agent.name}` : `Make ${agent.name} permanent`;
  const last = step === 2 || others.length === 0;

  return (
    <Modal
      title={title}
      onClose={onClose}
      footer={
        <>
          {step === 2 && (
            <Button type="button" variant="ghost" onClick={() => setStep(1)} disabled={busy}>
              Back
            </Button>
          )}
          <Button type="button" variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button type="button" variant="primary" disabled={busy} onClick={() => (step === 1 ? next() : void save())}>
            {busy ? "Saving..." : last ? "Save" : "Next"}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <p className="text-[12.5px] text-text-hint">
          Step {step} of {others.length === 0 ? 1 : 2}. A permanent agent stays to work on a standing objective, with or
          without people present. It may address only the agents you list, and must mention each one by name.
        </p>
        {step === 1 ? (
          <>
            <div className="flex flex-col gap-2">
              <SectionLabel>{agent.name} may converse with</SectionLabel>
              {others.length === 0 ? (
                <p className="text-sm text-text-dim">
                  There are no other permanent agents yet, so {agent.name} will not be allowed to address anyone. You
                  can change that when more join.
                </p>
              ) : (
                others.map((o) => (
                  <Checkbox
                    key={o.id}
                    checked={mine.includes(o.id)}
                    onChange={() => setMine(toggleId(mine, o.id))}
                    label={o.name}
                  />
                ))
              )}
            </div>
            <Field label="Objective" help="What it works toward, whether or not anyone is present. Optional.">
              <Textarea rows={3} value={objective} onChange={(e) => setObjective(e.target.value)} />
            </Field>
            <Field label="Wake it every (minutes)" help="Blank means only when addressed. From 5 to 1440.">
              <Input value={wake} onChange={(e) => setWake(e.target.value)} inputMode="numeric" />
            </Field>
          </>
        ) : (
          <div className="flex flex-col gap-4">
            <p className="text-sm text-text-dim">
              These permanent agents are already in the room. Should any of them be able to address {agent.name}, or
              each other? Lists work in one direction: letting A address B does not let B address A.
            </p>
            {others.map((o) => {
              const targets = [agent, ...others.filter((x) => x.id !== o.id)];
              return (
                <div key={o.id} className="flex flex-col gap-2">
                  <SectionLabel>{o.name} may converse with</SectionLabel>
                  {targets.map((t) => (
                    <Checkbox
                      key={t.id}
                      checked={(lists[o.id] ?? []).includes(t.id)}
                      onChange={() => setLists({ ...lists, [o.id]: toggleId(lists[o.id] ?? [], t.id) })}
                      label={t.id === agent.id ? `${t.name} (new)` : t.name}
                    />
                  ))}
                </div>
              );
            })}
          </div>
        )}
        {error && <ErrorText>{error}</ErrorText>}
      </div>
    </Modal>
  );
}
