import clsx from "clsx";
import { ChevronDown, ChevronRight } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { Link, useParams } from "react-router-dom";
import {
  api,
  ApiError,
  type JoinRequest,
  type Me,
  type Meeting,
  type MeetingDetail,
  type MeetingMessage,
  type MeetingParticipant,
  type MyJoinRequests,
  type TownhallAgent,
  type TurnMode,
} from "../lib/api";
import { dashPattern } from "../lib/dashPattern";
import { presenceView } from "../lib/meetingPresence";
import { type BubbleStyle, bubbleStyle, type Side, toneColor } from "../lib/bubbles";
import { canAssignPermanent, canDismiss } from "../lib/meetingRules";
import { roomGroups } from "../lib/roomGroups";
import { dollarsToCents } from "../lib/permanent";
import {
  groupRequests,
  isOpen,
  lineStatus,
  needsOwnersAnswer,
  requestLabel,
  type RequestGroup,
} from "../lib/waitingRoom";
import { InvitePane } from "./InvitePane";
import { MakePermanentDialog } from "./MakePermanentDialog";
import { RowMenu, type MenuItem } from "./RowMenu";
import { PageBody, PageHeader } from "./AppShell";
import { Button, Card, ErrorText, Input, Modal, SectionLabel, Select, StatusText, Tag } from "./ui";

/// How often an open meeting is refreshed. A realtime stream replaces this later
/// (see docs/townhall.md).
const POLL_MS = 1500;

/// How close to the end of the conversation counts as "reading the latest".
const STICK_TO_BOTTOM_PX = 80;

const TURN_MODES: { value: TurnMode; label: string }[] = [
  { value: "moderator_called", label: "only when called on" },
  { value: "mention_only", label: "when mentioned" },
  { value: "every_message", label: "on every message" },
];

function errorText(e: unknown, fallback: string): string {
  return e instanceof ApiError ? e.message : fallback;
}

function modeLabel(mode: TurnMode | null): string {
  return TURN_MODES.find((m) => m.value === mode)?.label ?? "only when called on";
}

/// The list of meetings, with a form to open a new one.
export function MeetingsPage() {
  const [meetings, setMeetings] = useState<Meeting[] | null>(null);
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setMeetings(await api.listMeetings());
      setError(null);
    } catch (e) {
      setError(errorText(e, "could not load meetings"));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function create(e: FormEvent) {
    e.preventDefault();
    if (!title.trim()) return;
    try {
      await api.createMeeting(title.trim());
      setTitle("");
      await refresh();
    } catch (err) {
      setError(errorText(err, "could not create the meeting"));
    }
  }

  return (
    <>
      <PageHeader title="Meetings" />
      <PageBody className="max-w-[760px]">
        <div className="flex flex-col gap-4">
          {error && <ErrorText>{error}</ErrorText>}
          <Card>
            <form onSubmit={create} className="flex gap-2">
              <Input
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder="Title of a new meeting"
                aria-label="Meeting title"
              />
              <Button type="submit" variant="primary" disabled={!title.trim()}>
                Open meeting
              </Button>
            </form>
          </Card>
          {meetings?.length === 0 && <p className="text-sm text-text-dim">No meetings yet.</p>}
          {meetings?.map((m) => (
            <Link key={m.id} to={`/meetings/${m.id}`}>
              <Card className="flex items-center justify-between hover:border-border">
                <span className="text-text-bright">{m.title}</span>
                <Tag>{m.status}</Tag>
              </Card>
            </Link>
          ))}
        </div>
      </PageBody>
    </>
  );
}

/// A titled panel the viewer can fold away. The choice is remembered per panel
/// in this browser; storage that is unavailable just means it opens expanded.
function CollapsiblePane({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  const key = `cityhall.meeting.pane.${id}`;
  const [open, setOpen] = useState(() => {
    try {
      return localStorage.getItem(key) !== "closed";
    } catch {
      return true;
    }
  });

  function toggle() {
    const next = !open;
    setOpen(next);
    try {
      localStorage.setItem(key, next ? "open" : "closed");
    } catch {
      // Not remembered; the pane still folds.
    }
  }

  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <div className="flex flex-col gap-3 rounded-card border border-border-soft bg-surface px-[22px] py-3">
      <button
        type="button"
        onClick={toggle}
        aria-expanded={open}
        className="flex w-full items-center justify-between gap-2 text-left focus-visible:outline-none"
      >
        <SectionLabel>{title}</SectionLabel>
        <Chevron size={14} className="shrink-0 text-text-dim" aria-hidden />
      </button>
      {open && children}
    </div>
  );
}

/// One meeting: the conversation on one scrolling pane, and on another the
/// panels for who is in the room, bringing agents, and the moderator's settings.
export function MeetingPage({ me }: { me: Me }) {
  const meetingId = Number(useParams().id);
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [messages, setMessages] = useState<MeetingMessage[]>([]);
  const [myAgents, setMyAgents] = useState<TownhallAgent[]>([]);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [waiting, setWaiting] = useState(false);
  const [requests, setRequests] = useState<JoinRequest[]>([]);
  const [mine, setMine] = useState<MyJoinRequests | null>(null);
  const [permanentFor, setPermanentFor] = useState<number | null>(null);
  const [working, setWorking] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<Confirmation | null>(null);
  const lastSeq = useRef(0);
  const scroller = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const [d, fresh] = await Promise.all([
        api.getMeeting(meetingId),
        api.listMeetingMessages(meetingId, lastSeq.current),
      ]);
      setDetail(d);
      setWaiting(false);
      if (fresh.length) {
        lastSeq.current = fresh[fresh.length - 1].seq;
        setMessages((prev) => [...prev, ...fresh]);
      }
      setError(null);
    } catch (e) {
      // Not in the meeting yet: the person sees the waiting room, not the room.
      if (e instanceof ApiError && e.status === 403) setWaiting(true);
      else setError(errorText(e, "could not load the meeting"));
    }
  }, [meetingId]);

  const loadRequests = useCallback(async () => {
    try {
      setRequests(await api.listJoinRequests(meetingId));
    } catch {
      // Only the moderator may read these; for anyone else the pane is not shown.
    }
  }, [meetingId]);

  const loadMyAgents = useCallback(() => {
    api
      .listMyTownhallAgents()
      .then(setMyAgents)
      .catch(() => {});
  }, []);

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(timer);
  }, [refresh]);

  const isModerator = detail?.moderator_user_id === me.id;
  useEffect(() => {
    if (!isModerator) return;
    void loadRequests();
    const timer = setInterval(() => void loadRequests(), POLL_MS);
    return () => clearInterval(timer);
  }, [isModerator, loadRequests]);

  useEffect(loadMyAgents, [loadMyAgents]);

  // What I asked for, was invited to, and invited: shown beside the room.
  const inMeeting = detail !== null;
  const loadMine = useCallback(async () => {
    try {
      setMine(await api.myJoinRequests(meetingId));
    } catch {
      // Shown only as a convenience; the room itself does not depend on it.
    }
  }, [meetingId]);
  useEffect(() => {
    if (!inMeeting) return;
    void loadMine();
    const timer = setInterval(() => void loadMine(), POLL_MS);
    return () => clearInterval(timer);
  }, [inMeeting, loadMine]);

  // Follow the conversation only while the reader is at its end, so scrolling
  // up to read is never yanked back down by a new message.
  useEffect(() => {
    const el = scroller.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;
  }, [messages.length]);

  function onScroll() {
    const el = scroller.current;
    if (el) pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_TO_BOTTOM_PX;
  }

  /// Runs a change and answers at once: the status line appears on the click
  /// itself, not when the server replies, and clears when the change settles.
  async function act(run: () => Promise<unknown>, fallback: string, doing = "Working...") {
    setWorking(doing);
    try {
      await run();
      setError(null);
      await refresh();
    } catch (e) {
      setError(errorText(e, fallback));
    } finally {
      setWorking(null);
    }
  }

  async function send(e: FormEvent) {
    e.preventDefault();
    const body = draft.trim();
    if (!body) return;
    setDraft("");
    pinned.current = true;
    await act(() => api.postMeetingMessage(meetingId, body), "could not send the message");
  }

  if (waiting) {
    return <WaitingScreen meetingId={meetingId} myAgents={myAgents} onAdmitted={() => void refresh()} />;
  }

  if (!detail) {
    return (
      <>
        <PageHeader title="Meeting" />
        <PageBody>
          {error ? <ErrorText>{error}</ErrorText> : <p className="text-sm text-text-dim">Loading...</p>}
        </PageBody>
      </>
    );
  }

  const inRoom = new Set(detail.participants.map((p) => p.agent_id));
  const bringable = myAgents.filter((a) => !inRoom.has(a.id));
  const mayEnrollUnasked = isModerator || detail.allow_member_agents;
  const groups = groupRequests(requests);
  const open = detail.status === "open";
  const room = roomGroups(detail.participants);
  const permanentTarget = detail.participants.find((p) => p.id === permanentFor) ?? null;
  const invitationsToMe = (mine?.items ?? []).filter(needsOwnersAnswer);
  const canAssign = canAssignPermanent(detail.participants, me.id, detail.moderator_user_id);

  async function changed() {
    await Promise.all([refresh(), loadRequests(), loadMine()]);
  }

  /// The ellipsis menu for a person or agent: only what this viewer may do.
  function menuFor(p: MeetingParticipant): MenuItem[] {
    const items: MenuItem[] = [];
    // An agent's own settings, for its owner.
    if (p.kind === "agent" && p.owner_user_id === me.id) {
      for (const m of TURN_MODES) {
        items.push({
          label: `Speaks ${m.label}`,
          checked: p.turn_mode === m.value,
          onSelect: () =>
            void act(
              () => api.setAgentTurnMode(meetingId, p.id, m.value),
              "could not change the turn mode",
              "Changing the turn mode...",
            ),
        });
      }
      items.push({
        label: p.visible_to_others ? "Hide from other people" : "Show to other people",
        onSelect: () =>
          void act(
            () => api.setVisibility(meetingId, p.id, !p.visible_to_others),
            "could not change who sees the agent",
            "Saving...",
          ),
      });
    }
    if (p.kind === "agent" && isModerator) {
      items.push({
        label: "Call on",
        onSelect: () => void act(() => api.callOnAgent(meetingId, p.id), "could not call on the agent"),
      });
    }
    // Permanent assignments belong to the moderator and the assistants they named.
    if (p.kind === "agent" && canAssign) {
      items.push({
        label: p.permanent ? "Permanent terms..." : "Make permanent...",
        onSelect: () => setPermanentFor(p.id),
      });
      if (p.permanent) {
        items.push({
          label: "End permanence",
          onSelect: () =>
            setConfirming({
              title: `End ${p.name}'s permanent residence?`,
              body: "Its conversation permissions are removed. It stays in the room as an ordinary agent.",
              action: "End permanence",
              run: () =>
                act(() => api.endPermanence(meetingId, p.id), "could not end permanence", "Ending permanence..."),
            }),
        });
      }
    }
    // Only the moderator names assistants, and only among the other people here.
    if (p.kind === "person" && isModerator && p.user_id !== me.id) {
      items.push({
        label: p.assistant ? "Remove as assistant moderator" : "Make assistant moderator",
        onSelect: () =>
          void act(() => api.setAssistant(meetingId, p.id, !p.assistant), "could not change the assistant"),
      });
    }
    if (canDismiss(p, me.id, detail!.moderator_user_id)) {
      items.push({
        label: "Dismiss",
        danger: true,
        onSelect: () =>
          setConfirming({
            title: `Dismiss ${p.name}?`,
            body: "They leave the meeting right away. An agent goes to the waiting room, where its owner or the moderator can bring it back.",
            action: "Dismiss",
            run: () =>
              act(() => api.dismissParticipant(meetingId, p.id), "could not dismiss", `Dismissing ${p.name}...`),
          }),
      });
    }
    return items;
  }

  function roomRow(p: MeetingParticipant) {
    return <ParticipantRow key={p.id} participant={p} all={detail!.participants} me={me} menu={menuFor(p)} />;
  }

  return (
    <>
      <PageHeader
        title={detail.title}
        meta={detail.status}
        actions={
          <Link to="/meetings" className="text-[13px] text-text-dim hover:text-text">
            All meetings
          </Link>
        }
      />
      <div className="flex min-h-0 flex-1 flex-col gap-4 px-[26px] pt-5 pb-5 md:flex-row">
        <section aria-label="Conversation" className="flex min-h-[55vh] min-w-0 flex-1 flex-col gap-3 md:min-h-0">
          {working && (
            <p role="status" className="flex items-center gap-2 text-[13px] text-text-dim">
              <span className="size-2 animate-pulse-dot rounded-full bg-waiting" aria-hidden />
              {working}
            </p>
          )}
          {error && <ErrorText>{error}</ErrorText>}
          <div
            ref={scroller}
            onScroll={onScroll}
            className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto rounded-card border border-border-soft bg-surface px-[22px] py-[18px]"
          >
            {messages.length === 0 && <p className="text-sm text-text-dim">Nothing said yet.</p>}
            {messages.map((m) => (
              <MessageRow key={m.seq} message={m} style={bubbleStyle(m, detail.participants, me.id)} />
            ))}
          </div>
          <form onSubmit={send} className="flex shrink-0 gap-2">
            <Input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              placeholder="Say something. Mention an agent as @Name."
              aria-label="Message"
              disabled={!open}
            />
            <Button type="submit" variant="primary" disabled={!draft.trim() || !open}>
              Send
            </Button>
          </form>
        </section>

        <aside
          aria-label="Room and moderator panels"
          className="flex max-h-[45vh] shrink-0 flex-col gap-3 overflow-y-auto md:max-h-none md:w-[280px] lg:w-[320px]"
        >
          {isModerator && (
            <CollapsiblePane id="moderator" title="Moderator">
              <div className="flex flex-col gap-3">
                <label className="flex flex-col gap-1 text-[13px] text-text-dim">
                  The most other people&apos;s agents may do
                  <Select
                    value={detail.agent_mode_ceiling}
                    onChange={(e) =>
                      act(
                        () => api.updateMeeting(meetingId, { agent_mode_ceiling: e.target.value as TurnMode }),
                        "could not change the ceiling",
                      )
                    }
                  >
                    {TURN_MODES.map((m) => (
                      <option key={m.value} value={m.value}>
                        speak {m.label}
                      </option>
                    ))}
                  </Select>
                </label>
                <label className="flex items-center gap-2 text-[13px] text-text-dim">
                  <input
                    type="checkbox"
                    checked={detail.allow_member_agents}
                    onChange={(e) =>
                      act(
                        () => api.updateMeeting(meetingId, { allow_member_agents: e.target.checked }),
                        "could not change the setting",
                      )
                    }
                  />
                  People in the meeting may bring their agents without asking
                </label>
                <HourlyCeiling
                  cents={detail.autonomous_hourly_cents}
                  onSave={(cents) =>
                    act(
                      () => api.updateMeeting(meetingId, { autonomous_hourly_cents: cents }),
                      "could not change the hourly ceiling",
                    )
                  }
                />
                <p className="text-[12.5px] text-text-hint">
                  Agents stop after {detail.max_consecutive_agent_turns} messages in a row with no person between (
                  {detail.max_consecutive_permanent_turns} for permanent agents).
                </p>
              </div>
            </CollapsiblePane>
          )}

          <CollapsiblePane id="room" title="In the room">
            <div className="flex flex-col gap-4">
              {room.permanent.length > 0 && (
                <RoomSection title="Permanent agents">{room.permanent.map(roomRow)}</RoomSection>
              )}

              {(room.people.length > 0 || room.strays.length > 0) && (
                <RoomSection title="People and their agents">
                  {room.people.map((g) => (
                    <div key={g.person.id} className="flex flex-col gap-2">
                      {roomRow(g.person)}
                      {g.agents.map((a) => (
                        <div key={a.id} className="ml-4">
                          {roomRow(a)}
                        </div>
                      ))}
                    </div>
                  ))}
                  {room.strays.map(roomRow)}
                </RoomSection>
              )}

              {isModerator && groups.length > 0 && (
                <RoomSection title={`Waiting list (${groups.length})`}>
                  <WaitingRoomPane
                    groups={groups}
                    onDecide={(item, approve) =>
                      act(
                        () => api.decideJoinRequest(meetingId, item.id, approve),
                        approve ? "could not approve" : "could not deny",
                      ).then(loadRequests)
                    }
                    onRemove={(item) =>
                      act(() => api.removeRequest(meetingId, item.id), "could not remove").then(loadRequests)
                    }
                  />
                </RoomSection>
              )}
            </div>
          </CollapsiblePane>

          <CollapsiblePane id="invite" title="Invite">
            <InvitePane
              meetingId={meetingId}
              isModerator={isModerator}
              participants={detail.participants}
              sent={mine?.sent ?? []}
              onChanged={() => void changed()}
            />
          </CollapsiblePane>

          <CollapsiblePane id="agents" title="Your agents">
            <AgentsPane
              bringable={bringable}
              mayEnrollUnasked={mayEnrollUnasked}
              onEnroll={(agent) =>
                act(() => api.enrollAgent(meetingId, agent.id, "mention_only"), "could not enroll the agent")
              }
              onAsk={(agent) =>
                act(
                  () => api.askToJoin(meetingId, [{ agent_id: agent.id, turn_mode: "mention_only" }]),
                  "could not ask the moderator",
                )
              }
              invitations={invitationsToMe}
              onRespond={(item, accept) =>
                act(() => api.respondToInvite(meetingId, item.id, accept), "could not answer the invitation").then(
                  () => void changed(),
                )
              }
            />
          </CollapsiblePane>
        </aside>
      </div>
      {permanentTarget && (
        <MakePermanentDialog
          meetingId={meetingId}
          agent={permanentTarget}
          participants={detail.participants}
          onClose={() => setPermanentFor(null)}
          onDone={() => void changed()}
        />
      )}
      {confirming && (
        <Modal
          title={confirming.title}
          onClose={() => setConfirming(null)}
          footer={
            <>
              <Button type="button" variant="ghost" onClick={() => setConfirming(null)}>
                Cancel
              </Button>
              <Button
                type="button"
                variant="danger"
                autoFocus
                onClick={() => {
                  const { run } = confirming;
                  setConfirming(null);
                  void run();
                }}
              >
                {confirming.action}
              </Button>
            </>
          }
        >
          <p className="text-sm text-text-dim">{confirming.body}</p>
        </Modal>
      )}
    </>
  );
}

/// A titled group inside the "In the room" pane.
function RoomSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <h3 className="font-mono text-[10.5px] tracking-[0.12em] text-text-hint uppercase">{title}</h3>
      {children}
    </div>
  );
}

/// A choice that needs a second click, shown in the page rather than in the
/// browser's own pop-up, which can be blocked and gives no sign it appeared.
interface Confirmation {
  title: string;
  body: string;
  action: string;
  run: () => Promise<void>;
}

/// Your registered agents that can join, and a pointer to where to make another.
function AgentsPane({
  bringable,
  mayEnrollUnasked,
  onEnroll,
  onAsk,
  invitations,
  onRespond,
}: {
  bringable: TownhallAgent[];
  /// The moderator, or anyone when the moderator lets people bring agents unasked.
  mayEnrollUnasked: boolean;
  onEnroll: (agent: TownhallAgent) => void;
  onAsk: (agent: TownhallAgent) => void;
  /// Invitations to bring one of your agents, which only you can accept.
  invitations: JoinRequest[];
  onRespond: (item: JoinRequest, accept: boolean) => void;
}) {
  const [asked, setAsked] = useState<Set<number>>(new Set());
  return (
    <div className="flex flex-col gap-4">
      {invitations.length > 0 && (
        <div className="flex flex-col gap-2">
          <SectionLabel>Invited</SectionLabel>
          {invitations.map((item) => (
            <div key={item.id} className="flex flex-col gap-1">
              <span className="text-sm text-text">
                {item.invited_by ?? "Someone"} invited {item.agent_name}
              </span>
              <span className="flex gap-2">
                <Button onClick={() => onRespond(item, true)}>Accept</Button>
                <Button variant="ghost" onClick={() => onRespond(item, false)}>
                  Decline
                </Button>
              </span>
            </div>
          ))}
        </div>
      )}
      {bringable.length > 0 && (
        <div className="flex flex-col gap-2">
          {bringable.map((a) => (
            <div key={a.id} className="flex items-center justify-between gap-2">
              <span className="truncate text-sm text-text" title={a.description}>
                {a.name}
              </span>
              {mayEnrollUnasked ? (
                <Button onClick={() => onEnroll(a)}>Enroll</Button>
              ) : asked.has(a.id) ? (
                <span className="text-[12.5px] text-text-hint">asked</span>
              ) : (
                <Button
                  onClick={() => {
                    setAsked(new Set(asked).add(a.id));
                    onAsk(a);
                  }}
                >
                  Ask to bring
                </Button>
              )}
            </div>
          ))}
        </div>
      )}
      {!mayEnrollUnasked && bringable.length > 0 && (
        <p className="text-[12.5px] text-text-hint">
          The moderator decides which of your agents join. Asking puts them in the waiting room.
        </p>
      )}
      <p className="text-[12.5px] text-text-hint">
        To make a new agent, or see the ones you have,{" "}
        <Link to="/agents" className="text-link hover:underline">
          open Agents
        </Link>
        .
      </p>
    </div>
  );
}

/// The moderator's waiting room: each person who is waiting, with the agents they
/// want to bring. Every line is approved or denied on its own, and a line left
/// alone just stays.
function WaitingRoomPane({
  groups,
  onDecide,
  onRemove,
}: {
  groups: RequestGroup[];
  onDecide: (item: JoinRequest, approve: boolean) => void;
  onRemove: (item: JoinRequest) => void;
}) {
  if (groups.length === 0) return <p className="text-[12.5px] text-text-hint">Nobody is waiting.</p>;
  return (
    <div className="flex flex-col gap-3">
      {groups.map((g) => (
        <div key={g.userId} className="flex flex-col gap-2">
          <BorderedBox agent={false} seed={g.userId} className="flex flex-col gap-1 px-3 py-2">
            <div className="flex items-center justify-between gap-2">
              <span className="truncate text-sm text-text">
                {g.username}
                {g.inRoom && <span className="text-text-faint"> (in the room)</span>}
              </span>
              {g.person && <DecisionButtons item={g.person} onDecide={onDecide} onRemove={onRemove} />}
            </div>
            {g.person && <span className="text-[12.5px] text-text-hint">{lineStatus(g.person)}</span>}
          </BorderedBox>
          {g.agents.map((a) => {
            // An invited agent waits for its owner's yes before the moderator can decide.
            const awaitingOwner = needsOwnersAnswer(a);
            return (
              <BorderedBox key={a.id} agent seed={a.agent_id ?? a.id} className="ml-4 flex flex-col gap-1 px-3 py-2">
                <div className="flex items-center justify-between gap-2">
                  <span className="truncate text-sm text-text">{a.agent_name}</span>
                  <DecisionButtons
                    item={a}
                    onDecide={onDecide}
                    onRemove={onRemove}
                    disableApprove={awaitingOwner || (a.kind === "ask" && !g.inRoom)}
                  />
                </div>
                <span className="text-[12.5px] text-text-hint">
                  {lineStatus(a)} · would speak {modeLabel(a.turn_mode)}
                  {awaitingOwner && " · waiting for its owner"}
                  {a.kind === "ask" && !g.inRoom && " · admit the person first"}
                </span>
              </BorderedBox>
            );
          })}
        </div>
      ))}
    </div>
  );
}

function DecisionButtons({
  item,
  onDecide,
  onRemove,
  disableApprove,
}: {
  item: JoinRequest;
  onDecide: (item: JoinRequest, approve: boolean) => void;
  onRemove: (item: JoinRequest) => void;
  disableApprove?: boolean;
}) {
  const label = requestLabel(item);
  return (
    <span className="flex shrink-0 items-center">
      <Button
        variant="ghost"
        disabled={disableApprove}
        aria-label={`Approve ${label}`}
        onClick={() => onDecide(item, true)}
      >
        Approve
      </Button>
      <Button variant="ghost" aria-label={`Deny ${label}`} onClick={() => onDecide(item, false)}>
        Deny
      </Button>
      <RowMenu label={label} items={[{ label: "Remove", danger: true, onSelect: () => onRemove(item) }]} />
    </span>
  );
}

/// The most that agents working with no person prompting them may spend in an
/// hour, in dollars. Saved when the box is left, so typing is not interrupted.
function HourlyCeiling({ cents, onSave }: { cents: number; onSave: (cents: number) => void }) {
  const [text, setText] = useState((cents / 100).toFixed(2));
  useEffect(() => setText((cents / 100).toFixed(2)), [cents]);
  const parsed = dollarsToCents(text);
  return (
    <label className="flex flex-col gap-1 text-[13px] text-text-dim">
      Hourly ceiling on agents working unprompted ($)
      <Input
        value={text}
        inputMode="decimal"
        aria-invalid={parsed === null}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => {
          if (parsed === null) setText((cents / 100).toFixed(2));
          else if (parsed !== cents) onSave(parsed);
        }}
      />
    </label>
  );
}

/// What someone sees before the moderator lets them in: the meeting's title, a
/// way to ask to join with some of their agents, and how each request stands.
function WaitingScreen({
  meetingId,
  myAgents,
  onAdmitted,
}: {
  meetingId: number;
  myAgents: TownhallAgent[];
  onAdmitted: () => void;
}) {
  const [info, setInfo] = useState<MyJoinRequests | null>(null);
  const [chosen, setChosen] = useState<Set<number>>(new Set());
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const mine = await api.myJoinRequests(meetingId);
      setInfo(mine);
      if (mine.member) onAdmitted();
    } catch (e) {
      setError(errorText(e, "could not load the waiting room"));
    }
  }, [meetingId, onAdmitted]);

  useEffect(() => {
    void load();
    const timer = setInterval(() => void load(), POLL_MS);
    return () => clearInterval(timer);
  }, [load]);

  const pending = info?.items.some((i) => i.status === "pending") ?? false;

  async function remove(item: JoinRequest) {
    try {
      await api.removeRequest(meetingId, item.id);
      await load();
    } catch (e) {
      setError(errorText(e, "could not withdraw"));
    }
  }

  async function ask() {
    try {
      await api.askToJoin(
        meetingId,
        [...chosen].map((agent_id) => ({ agent_id, turn_mode: "mention_only" as const })),
      );
      setChosen(new Set());
      setError(null);
      await load();
    } catch (e) {
      setError(errorText(e, "could not ask to join"));
    }
  }

  return (
    <>
      <PageHeader
        title={info?.title ?? "Meeting"}
        meta="waiting room"
        actions={
          <Link to="/meetings" className="text-[13px] text-text-dim hover:text-text">
            All meetings
          </Link>
        }
      />
      <PageBody className="max-w-[560px]">
        <div className="flex flex-col gap-4">
          {error && <ErrorText>{error}</ErrorText>}
          <Card className="flex flex-col gap-3">
            <SectionLabel>Ask to join</SectionLabel>
            <p className="text-sm text-text-dim">
              The moderator decides who comes in. You can bring agents of your own, and each is decided separately.
            </p>
            {myAgents.length > 0 && (
              <div className="flex flex-col gap-1.5">
                {myAgents.map((a) => (
                  <label key={a.id} className="flex items-center gap-2 text-sm text-text">
                    <input
                      type="checkbox"
                      checked={chosen.has(a.id)}
                      onChange={(e) => {
                        const next = new Set(chosen);
                        if (e.target.checked) next.add(a.id);
                        else next.delete(a.id);
                        setChosen(next);
                      }}
                    />
                    {a.name}
                  </label>
                ))}
              </div>
            )}
            <Button variant="primary" onClick={() => void ask()} disabled={pending && chosen.size === 0}>
              {pending ? "Add to my request" : "Ask to join"}
            </Button>
          </Card>
          {info && info.items.length > 0 && (
            <Card className="flex flex-col gap-2">
              <SectionLabel>Your requests</SectionLabel>
              {info.items.map((item) => (
                <div key={item.id} className="flex items-center justify-between gap-2 text-sm">
                  <span className="truncate text-text">
                    {requestLabel(item)}
                    {item.agent_id !== null && <span className="text-text-faint"> (agent)</span>}
                  </span>
                  <span className="flex shrink-0 items-center gap-1">
                    <span
                      className={
                        item.status === "denied" || item.status === "declined"
                          ? "text-error"
                          : item.status === "approved"
                            ? "text-running"
                            : "text-text-hint"
                      }
                    >
                      {lineStatus(item)}
                    </span>
                    {isOpen(item) && (
                      <Button variant="ghost" onClick={() => void remove(item)}>
                        Withdraw
                      </Button>
                    )}
                  </span>
                </div>
              ))}
            </Card>
          )}
        </div>
      </PageBody>
    </>
  );
}

/// A thin box that tells people from agents at a glance: a person's is solid, an
/// agent's is dashed in a pattern of its own, the same wherever that agent
/// appears (its messages and its entry in the room list).
function BorderedBox({
  agent,
  seed,
  className,
  color,
  children,
}: {
  agent: boolean;
  seed: number;
  className?: string;
  /// A CSS color for the border; the fill is the same color at 15%. Left out, the
  /// box keeps the plain border and no fill.
  color?: string;
  children: ReactNode;
}) {
  const stroke = color ?? "var(--color-border)";
  const fill = color ? `color-mix(in srgb, ${color} 15%, transparent)` : "none";
  if (!agent) {
    return (
      <div
        className={clsx("rounded-md border", className)}
        style={{ borderColor: stroke, backgroundColor: color ? fill : undefined }}
      >
        {children}
      </div>
    );
  }
  return (
    <div className={clsx("relative rounded-md", className)}>
      <svg aria-hidden className="pointer-events-none absolute inset-0 h-full w-full overflow-visible">
        <rect
          x="0.5"
          y="0.5"
          rx="5.5"
          style={{ width: "calc(100% - 1px)", height: "calc(100% - 1px)" }}
          fill={fill}
          stroke={stroke}
          strokeWidth="1"
          strokeDasharray={dashPattern(seed)}
        />
      </svg>
      {children}
    </div>
  );
}

const SIDE_CLASS: Record<Side, string> = {
  right: "self-end",
  center: "self-center",
  left: "self-start",
};

/// One box in the conversation. It is 85% of the panel wide, sits right,
/// center or left by who spoke, keeps its text left-aligned, and carries its label
/// on the top border at the box's left edge, whichever side the box sits on.
function MessageRow({ message, style }: { message: MeetingMessage; style: BubbleStyle }) {
  const hidden = message.dialog_type === "thought" && !message.revealed;
  const kind = message.dialog_type !== "contribution" ? message.dialog_type.replace(/_/g, " ") : null;
  return (
    <div className={clsx("flex w-[85%] flex-col pt-2", SIDE_CLASS[style.side])}>
      <div className="relative w-full">
        <span
          className={clsx(
            "absolute top-0 left-3 z-10 flex max-w-[calc(100%-1.5rem)] -translate-y-1/2 items-center gap-1.5 bg-surface px-1.5 text-[12px] whitespace-nowrap text-text-dim",
          )}
        >
          <span className="truncate font-medium text-text-bright">{message.speaker}</span>
          {message.is_agent && <Tag>agent</Tag>}
          {kind && <Tag accent>{kind}</Tag>}
        </span>
        <BorderedBox
          agent={message.is_agent}
          seed={message.participant_id}
          color={toneColor(style.tone)}
          className="px-3 pt-3 pb-2"
        >
          <p
            className={clsx(
              "text-left text-sm break-words whitespace-pre-wrap",
              hidden ? "font-mono text-text-faint" : "text-text",
            )}
          >
            {message.body}
          </p>
        </BorderedBox>
      </div>
    </div>
  );
}

function ParticipantRow({
  participant,
  all,
  me,
  menu,
}: {
  participant: MeetingParticipant;
  all: MeetingParticipant[];
  me: Me;
  /// What the viewer may do with this person or agent, in its ellipsis menu.
  menu: MenuItem[];
}) {
  if (participant.kind === "person") {
    return (
      <BorderedBox agent={false} seed={participant.id} className="flex items-center justify-between gap-2 px-3 py-2">
        <span className="truncate text-sm text-text">
          {participant.name}
          {participant.user_id === me.id && <span className="text-text-faint"> (you)</span>}
          {participant.assistant && <span className="text-text-faint"> (assistant moderator)</span>}
        </span>
        <RowMenu label={participant.name} items={menu} />
      </BorderedBox>
    );
  }
  const view = presenceView(participant.presence, all);
  const mine = participant.owner_user_id === me.id;
  const clamped = participant.turn_mode !== participant.effective_turn_mode;
  return (
    <BorderedBox agent seed={participant.id} className="flex flex-col gap-1.5 px-3 py-2">
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-sm text-text">
          {participant.name}
          {participant.permanent && <span className="text-text-faint"> (permanent)</span>}
        </span>
        <RowMenu label={participant.name} items={menu} />
      </div>
      <StatusText tone={view.tone} glyph={view.glyph} title={view.title}>
        {view.label}
      </StatusText>
      {participant.permanent && (
        <div className="flex flex-col gap-0.5 text-[12.5px] text-text-hint">
          {participant.objective && <span title={participant.objective}>Objective: {participant.objective}</span>}
          <span>
            May address:{" "}
            {participant.may_address.length ? participant.may_address.map((a) => a.name).join(", ") : "no one"}
          </span>
          {participant.wake_every_minutes && <span>Woken every {participant.wake_every_minutes} min</span>}
        </div>
      )}
      <span className="text-[12.5px] text-text-hint">
        speaks {modeLabel(participant.effective_turn_mode)}
        {mine && !participant.permanent && !participant.visible_to_others && " · hidden from other people"}
      </span>
      {clamped && (
        <span className="text-[12.5px] text-waiting">
          limited by the moderator to speak {modeLabel(participant.effective_turn_mode)}
        </span>
      )}
    </BorderedBox>
  );
}
