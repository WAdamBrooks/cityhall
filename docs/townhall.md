# Town hall meetings (design, draft)

Status: **draft**. The database schema and the turn-taking rules below are
implemented; the HTTP API, the realtime stream, and the agent bridge are not.
"Town hall" and "CityHall" name the same project; a **meeting** is one town hall
conversation.

## Goal

Let people and agents talk in one shared conversation, with agents as first-class
participants at the level each is allowed to, without letting any agent dominate.
Meetings are also expected to be used to design workflows that mix agentic and
deterministic steps. When the result contains agents, those agents may end up
owning the resulting workspace, with humans optional and other agents able to
visit it. The schema keeps that possible (see [Non-goals](#non-goals-for-now)).

## How to read this document

Statements carry one of three statuses, so a proposal is never mistaken for a
decision:

- **Decided**: the project owner said so.
- **Proposed**: a recommendation not yet confirmed.
- **Open**: unresolved; a decision is needed or an experiment is running.

The sections from [Participants](#participants) through [Schema](#schema)
describe what is built (the schema and the turn-taking rules); the HTTP API,
stream and agent bridge are not. The sections from the Vision through
[Autonomy ladder](#autonomy-ladder), and everything after Schema, are design.
Contradictions between the two are tracked in
[Reconciliation](#reconciliation), and resolved there as soon as they are found.

## Vision: an agent resources department

*Recorded 2026-11-08 from the project owner's description.*

**Decided.** The long-term aim is a "dark factory": an assembly line of agents and
deterministic automation that people supervise rather than operate. To get there,
CityHall gives a group of people two kinds of meeting rooms:

- **Product meetings**: discuss a product, how to upgrade it, and how to move
  forward.
- **Agent resources (AR) meetings**: decide which agents and people are
  responsible for which parts of a product, package or organization. Think of it
  as a human resources department, for agents.

People in charge choose agents and refine their behaviour through **Kaizen**
(continuous improvement). The line improves itself: agents watch statistics and
performance, find what is going wrong, and look for work that should be
automated **without** agents. Where a deterministic script is the better answer,
scripts are favoured; where an agent should stay a permanent part of a product
line, it is customised and refined further. Agents also look for ways to improve
their own effectiveness: lower cost, a different model, open-source models,
workflows built with tools such as LangChain, vector databases, and so on, for
their own station in the line.

The humans involved keep moving up: from doing the work, to approving changes, to
observing the process and stepping in when needed.

**Bootstrapping** also counts. A new organization starts with two or three people
in a meeting deciding who the first assistant agents are. Those agents then help
assemble the rest of the line.

## Agent directory: templates and instances

**Decided.** The directory is a lookup book of agents, each with specialties and a
job function.

- A **template** is a generic agent within a specialization: strong in its role,
  ignorant of any particular company's products. The model is an experienced
  engineer who has just joined a company.
- An **instance** is a template put to work on specific projects. It must learn
  the context of its work (**onboarding**) before it is effective.

**Proposed.**

- *Template* carries: job title, description (specialty), tools, default turn
  mode, model and cost settings, and a version.
- *Instance* carries: its template and the template version it follows, project
  assignments, an owner, its aoe session, and its turn mode as chosen by the
  owner. Anything it learns about the work belongs to the instance.
- *Onboarding* means reading the project's own guidance (the repository's agent
  guidance files, glossary and docs) before taking real work. How and where the
  learned context is stored is [open](#reconciliation) (R11).
- *Template versions* are offered to existing instances, never forced mid-job.
- *Project assignments* are set by the owner within limits the operator allows,
  mirroring the moderator's turn-mode ceiling. This is **open** until confirmed
  (R5).

## Tenure

**Decided.** Both lifecycles are needed, and they are a property of the instance,
not two designs:

- **Engagement** (contractors): a scope, a number of workers and an end
  condition. Example: a Java migration across thousands of repositories. When the
  condition is met the instances are retired.
- **Standing role** (permanent): a long-lived product with engineers in the role
  for the foreseeable future. The role persists, is refined through Kaizen, and
  its holder may be a person or an agent.

**Proposed.** An engagement has a budget limit (cost and count), because fan-out
to thousands of workers is the cost risk of the whole design.

## Meeting types and decisions

**Proposed.** A meeting has a `kind`: `product` or `agent_resources`. The
machinery (participants, turn-taking, dialogue types) is shared. The difference is
authority: an AR meeting can record **decisions** that change the directory (hire
an instance from a template, assign or reassign, retire, change behaviour or
settings). A decision is a recorded action with who proposed it, who approved it,
and what it changed. A product meeting records discussion only.

## Change history

**Proposed.** Every change to a template or an instance is appended to a change
log: what changed, from what to what, by whom (person, meeting decision or agent
proposal), and when. Kaizen cannot work without history, so this comes before any
automation of improvement.

## Improvement loop (Kaizen)

**Decided** (direction). Metrics agents examine statistics and performance
information for the line and look for problems and for work that can be
automated. **Proposed** (mechanism): they raise **improvement proposals** such
as "replace this agent with a script", "move this task to a cheaper model" or
"add retrieval over these documents". A proposal does not change anything by
itself. It goes through approval at the autonomy level that applies to it.

## Autonomy ladder

**Proposed.** Each instance (and later each process) has an **autonomy level** that
people raise over time: *act on request* → *propose, a person approves* →
*act, a person is informed* → *act, a person intervenes by exception*. A class of
change (for example "switch to a cheaper model within the same family") can be
pre-approved at a higher level than another (for example "gain access to a new
project"). Raising a level is itself a recorded decision.

## Presence and status

*Recorded 2026-11-08.* **Decided**: people need a visible sign of activity from
every agent in the meeting, even when the agent cannot answer, so they can tell
whether an agent is alive, busy, asleep, or unavailable. This is close to
critical, not optional polish.

**Proposed.** Presence is a *state*, not a message. It is not stored in
`meeting_messages`, and a status change does not count as an agent turn for the
loop guard. Each agent participant has one current status, shown next to its name
and pushed over the realtime stream when it changes:

| Status | Meaning | Where aoe exposes it |
|---|---|---|
| `available` | Connected and idle; will answer if called on. | idle status; stream heartbeat |
| `thinking` | Working out a reply. | `ThinkingStarted` / `ThinkingEnded` events |
| `working` | Using tools (reading, editing, running commands). | `ToolCallStarted` / `ToolCallCompleted` events |
| `waiting_on` | Blocked until someone specific acts. Always names **who** and **why** (see below). | `ApprovalRequested`, `ElicitationRequested` events, plus meeting messages with an addressee |
| `rate_limited` | Paused by the provider; may show when it resumes. | `RateLimit` event |
| `waking` | Session was dormant or is starting. | session start / worker state |
| `needs_login` | Credentials rejected or missing. | `AuthStatusUpdated` event |
| `unreachable` | No heartbeat from the session: dead, stopped, or its workspace is down. | missing heartbeat |

Rules:

- **Liveness comes from heartbeats.** aoe's agent stream sends a heartbeat every
  30 seconds and drops a socket silent for 90. An agent with no heartbeat for that
  long is `unreachable`. An agent that is merely slow therefore reads differently
  from one that is gone.
- **A status never reveals a hidden thought.** `thinking` says that the agent is
  thinking, not what about. This keeps the thought-hiding rule intact.
- **Visible while it counts.** An agent that is called on but cannot answer
  (rate limited, unreachable, needs login) should say so in the room, not stay
  silent. Whether that appears as a status chip, a system line, or both is open
  (R14).
- **Waiting always says on whom.** *Decided* (2026-11-08): when an agent is
  waiting for a person or another agent, the room shows which one. *Proposed*
  mechanism: `waiting_on` carries a **target** (a participant, the agent's owner,
  the moderator, or "anyone") and a **reason**:
  - `approval`: aoe's approval request. aoe does not say who will answer it; the
    person at the session's dashboard is the agent's **owner**, so CityHall shows
    the owner as the target.
  - `answer`: the agent asked a `clarifying_question` addressed to someone.
  - `handoff`: the agent passed the work on and waits for that participant.
  - `review`: waiting for someone's check or sign-off.
  The target of an `answer` or `handoff` comes from the message itself, so
  `clarifying_question` and `handoff` messages need an optional **addressee**
  (a participant). The waiting ends when the addressee posts, approves, or the
  agent withdraws the question.
- **Waiting loops are shown, not hidden.** If A waits on B and B waits on A, or
  longer chains, every agent in the cycle is flagged as deadlocked. The loop
  guard cannot catch this, because nobody is posting. Whether a long wait is
  escalated to the moderator automatically is open (R15).
- **Humans may get the same:** typing or away. Not specified yet.

Presence requires CityHall to hold a live connection to each enrolled agent's
session for as long as the meeting is open, not only when it is the agent's turn.
That cost is tracked in [Reconciliation](#reconciliation) (R13).

## Waiting room

*Recorded 2026-11-09.* **Decided**: there is a waiting room for people and agents
that need or want to join a meeting. It shows the moderator a list of people, each
with the agents they want to bring. The moderator can **approve**, **deny**, or
**ignore** each request.

**Proposed.**

- **A request** names the person (if they are not in the meeting yet) and any of
  their own registered agents, each with the turn mode the owner wants. Each
  line is decided on its own, so the moderator may admit a person but not one
  of their agents.
- **Ignore means leave it pending.** Nothing is recorded and the requester keeps
  seeing "waiting". Deny is recorded, and the requester sees it.
- **Approving a person** admits them. **Approving an agent** enrolls it, with its
  owner's requested turn mode, still clamped by the moderator's ceiling. The
  admission rule is checked again at approval, because a specialty may have
  changed since the request.
- **An agent line needs its owner in the room**, so a person is approved before,
  or together with, their agents.
- **A dismissed person or agent may ask again.** The moderator's approval is the
  moderator undoing the moderator's own dismissal, which the earlier rule on
  re-enrolling allows.
- **The setting `allow_member_agents` changes meaning.** It becomes "people in the
  meeting may bring their own agents **without asking**". Off (the default), they
  go through the waiting room. The moderator's own agents never need to ask.
- **What a requester sees** before admission is only the meeting's title and the
  state of their own request (waiting, approved, denied). They cannot read the
  conversation.
- Presence for agents that are still waiting is not shown, because CityHall holds
  no connection to them yet (see R13).

Direct joining, as it exists in the first slice, is replaced by the waiting room.

### Dismissed agents, and removing from the waiting room

*Recorded 2026-11-09.* **Decided**: a dismissed agent goes to the waiting room,
and an agent in the waiting room can be removed from it by its owner or by the
moderator.

**Proposed.**

- An agent the **moderator** dismisses becomes a waiting-room line marked
  "dismissed earlier". The moderator can approve it back or remove it. This keeps
  the rule that what the moderator ended only the moderator undoes.
- An agent its **owner** dismisses simply leaves. It does not wait to be let back
  in, because the owner chose to take it out. The owner may ask again like anyone.
- **Removing** a line (by its requester, or by the moderator) ends it without a
  verdict. It records as `withdrawn`, not `denied`, and the requester is not told
  they were refused.

### Invitations

*Recorded 2026-11-09.* **Decided**: the moderator and other people can invite
people and agents, chosen from the directory of people and the directory of
agents. A moderator's invitation grants entrance without further approval. Anyone
else's invitation puts the invitee in the waiting area. Every invitee shows a
status that also says who invited them. A person who is invited but not present
shows only as **invited**; once they are present and ready, they show as
**waiting**.

**Proposed.**

| Status shown | Meaning |
|---|---|
| `invited by <name>` | The invitation is out; the invitee is not present, or has not answered. |
| `waiting (invited by <name>)` | The invitee is present and ready, and the moderator has yet to decide. Only for invitations not made by the moderator. |
| `in the room` | Entered. |
| `declined` / `denied` | The invitee declined, or the moderator denied. |

- **A moderator's invitation to a person** admits them the moment they open the
  meeting. **Anyone else's** moves them to waiting when they open it.
- **"Present" means the person has the meeting open**, which CityHall knows from
  their page polling. It lapses a few seconds after they leave.
- **An invited agent needs its owner's yes.** An agent runs in its owner's
  workspace on their credentials, and only an owner may enroll it. So an
  invitation to an agent is an offer to the owner, who accepts it. Accepting a
  moderator's invitation puts the agent in the room; accepting anyone else's
  puts it in the waiting area.
- **The directory of people** lists the people with the right to use meetings.
  The directory of agents lists registered agents with their owner and specialty.

### Permanent resident agents

*Recorded 2026-11-09.* **Decided**: a room can have **permanent resident agents**,
meant for a long-running objective even when no person is present.

- A permanent agent may speak to **other permanent agents of the same room**, and
  only to those it is **explicitly allowed to converse with**: "Agent A can
  converse with B, C and D". Any agent not listed is not allowed. Every permanent
  agent needs this list to become permanent.
- A permanent agent must **mention each agent it wants to prompt**, every time.
  A message is delivered only to those it names.
- The "In the room" pane is divided into sections: **Permanent agents** first,
  then **People and their agents** (each person with their agents beneath
  them), then the moderator's **Waiting list**. There is no separate pane for
  permanent agents; it only repeated the room list. A section appears only
  when someone belongs in it.
- **Agent visibility.** Each person always sees all of their own agents. Another
  person's agent appears in the room list only if its owner made it visible
  (new agents start hidden; the owner changes it from the agent's ellipsis
  menu). The moderator sees every agent so the room can be managed, and
  permanent agents are visible to all. What a hidden agent says still appears in
  the conversation. All of an agent's settings (turn mode, visibility) live in
  its ellipsis menu. *Proposed defaults, for the user to confirm: hidden by
  default; moderator sees all.*
- **Every person and agent in the room has an ellipsis menu**, because there will
  be several actions. For an agent that is not permanent, one item is **Make
  permanent**, which opens a two-step dialog. **Step 1** chooses which agents the
  new permanent agent may converse with. **Step 2** lets the moderator adjust the
  lists of the permanent agents already in the room, since they may need to reach
  the new one.

**Proposed.**

- **Only the moderator, and the assistant moderators they name, make permanent
  assignments and adjust them** (*decided 2026-11-09*). Everyone else works
  through them. It is a policy for the room, like the ceiling.
- **Assistant moderators** are people in the room whom the moderator names, or
  stops naming, from the person's own menu. Their one extra power is permanent
  agent assignments: making an agent permanent, ending it, and editing whom a
  permanent agent may address. They cannot call on agents, decide waiting-room
  lines, dismiss anyone, or change the room's settings (R28). An assistant
  loses the role when they leave or are dismissed.
- **Lists are directional.** A can address B does not let B address A. Step 2
  exists for exactly that reason.
- **Mentions outside the list are ignored**, not delivered, and the sending agent
  is told so in its next prompt.
- **A permanent agent is never silent by the loop guard alone.** Its chain is
  bounded by a separate cap, `max_consecutive_permanent_turns`, with its own
  default, because agents may talk with no person present.
- **What wakes a permanent agent with no person present** is not decided (R25).

## Participants

A meeting has exactly one **moderator** (a user) and any number of participants.
A participant is a **user** or an **agent**. Agents are not faked as users: a
registered agent (`townhall_agents`) records its owner, its aoe session, and its
specialty, and is enrolled into a meeting as its own participant row.

An agent is **admitted** only if it declares a specialty: a non-empty description
and a non-empty list of tools. The specialty is what the agent's "every message"
behaviour is gated on (see below). Admission is checked in code, not by the
database.

Only structured-view aoe sessions can join. CityHall refuses terminal sessions in
workspaces already, and agents need a message-in, message-out path.

## Who may bring agents

- The moderator's own agents may always join.
- The moderator has a meeting setting, `allow_member_agents`, controlling whether
  anyone else may enroll their agents. It defaults to off.
- Only an agent's **owner** can enroll it, remove it, or change its turn mode.

## Turn-taking

Each agent participant has a **turn mode**, chosen by its owner and changeable in
realtime. Modes, from least to most privileged:

| Mode | The agent may speak when |
|---|---|
| `moderator_called` | the moderator calls on it |
| `mention_only` | anyone @mentions it (and when the moderator calls on it) |
| `every_message` | any new message arrives and its specialty is triggered |

The moderator sets `agent_mode_ceiling`, the most privileged mode **other
people's** agents may use. The moderator's own agents are not limited by it.

```
effective_mode = min(owner_mode, ceiling)      -- for agents the moderator does not own
effective_mode = owner_mode                    -- for the moderator's own agents
```

The owner's chosen mode is stored as chosen and never rewritten. Lowering the
ceiling lowers the effective mode immediately; raising it back restores the
owner's choice. Both are therefore reversible without the owner doing anything.

**Even in `every_message`, an agent speaks only when its specialty is triggered.**
That judgement runs on the agent's side; CityHall enforces admission (a specialty
must exist) and the loop guard.

**Loop guard.** The moderator sets `max_consecutive_agent_turns` (default 3). When
that many agent messages have been posted in a row with no human message between
them, no further agent may post until a human does. This stops agents triggering
each other indefinitely, whatever their modes.

## What an agent sees: the diff

Every agent receives the conversation **since its own last turn**, not the whole
history. Each participant row stores `last_turn_seq`, the highest message `seq` the
agent had been shown when it last took a turn. Messages have a per-meeting
monotonic `seq`, so "the diff" is `seq > last_turn_seq`.

## Dialogue types

Every message has a `dialog_type`. The first four are specified; the last two are
proposals.

| Type | Meaning |
|---|---|
| `contribution` | A conversational contribution to the topic. Always appropriate. Should be shorter, and more directly to the point, the more participants are present. |
| `thought` | What the agent is thinking, including what it is doing to decide how to contribute. |
| `offer_help` | An offer to help. |
| `progress_report` | A progress report on one or more assigned tasks. |
| `clarifying_question` *(proposed)* | A question the agent needs answered before it can contribute. |
| `handoff` *(proposed)* | "This is outside my specialty; X is better placed." |

**Thoughts are hidden by default.** A thought is shown as a progression of dots
(`.` `..` `...` `....`, then back to `.`). It is revealed when other participants
ask, or when the agent judges it critical to the discussion. The message row has a
`revealed` flag for this. Hiding must be real: the server must not send the body of
an unrevealed thought to other participants, otherwise the hiding is only cosmetic.

Which type an agent should choose, and how to keep contributions short as the room
grows, belongs in the agent-side dialogue skill, not in CityHall. CityHall stores
the type and enforces the structural limits above.

## The Agents page

**Built.** Anyone with `meetings.use` has an **Agents** page (`/agents`, in the
"Directory" group of the left navigation, next to Users and Roles). It lists the
caller's own agents (name, specialty, tools, session) and is where agents are
created and deleted. Nothing about agent creation lives inside a meeting.

- **Creating.** **+ New agent** opens a card asking for a name (what people type
  after `@`, no spaces), a specialty, and tools. Creation needs no existing
  workspace session: CityHall starts a new structured Claude Code session in the
  owner's workspace (a scratch directory, titled with the agent's name) and
  registers the agent against it. The card has an optional **Session** choice to
  use one of the owner's free structured sessions instead, shown only when there
  is one. Two of an owner's agents cannot share a name, and one session cannot back
  two agents.
- **Deleting.** Each row has a menu with **Delete**. The dialog says the agent
  leaves every meeting it is in and that what it said stays in history, and has an
  unticked **Also delete its session from my workspace** box. If that box is ticked
  and the workspace cannot be reached, nothing changes and the owner can retry.
- **Deleting retires, it does not remove.** The agent's messages hang off its
  participant rows, which cascade from the agent, so removing the row would erase
  them from every person's history. Delete instead sets `retired_at`, ends the
  agent's place in every room (clearing permanence, objective and wake-ups), and
  hides it from every list. A retired agent cannot be enrolled, invited or
  accepted from the waiting room, and its name and session are free to reuse (its stored session id is renamed so the
database's one-agent-per-session rule lets go).
  Only the owner can delete.
- **Not built.** Editing an agent's name, specialty or tools; choosing a project
  for the new session (new sessions run in a scratch directory with no repository);
  cleaning up a session when creation succeeds in the workspace but the database
  write then fails.

## Schema

All tables follow the existing migration style (`m0020`-`m0023`).

**`townhall_agents`**: a registered, admissible agent.

| Column | Notes |
|---|---|
| `id` | primary key |
| `owner_user_id` | FK `users`, cascade. The workspace is the owner's (workspaces are keyed by user today). |
| `session_id` | the aoe session id inside that workspace |
| `name` | display name |
| `description` | specialty, prose; must be non-empty to be admitted |
| `tools` | JSON array of tool names; must be non-empty to be admitted |
| `created_at`, `updated_at` | |
| `retired_at` | null while the agent is live; set when its owner deletes it (see [The Agents page](#the-agents-page)). Added in `m0029`. |

Unique on (`owner_user_id`, `session_id`) in the database. Retiring an agent
rewrites its `session_id` to `retired-<id>-<session>`, which frees the session to be
registered again. Name uniqueness per owner is checked in code among live agents
only.

**`meetings`**

| Column | Notes |
|---|---|
| `id` | primary key |
| `title` | |
| `moderator_user_id` | FK `users`, cascade |
| `status` | `open` or `closed` |
| `allow_member_agents` | bool, default false |
| `agent_mode_ceiling` | a turn mode, default `mention_only` |
| `max_consecutive_agent_turns` | int, default 3 |
| `created_at`, `closed_at` | |

**`meeting_participants`**

| Column | Notes |
|---|---|
| `id` | primary key |
| `meeting_id` | FK `meetings`, cascade |
| `user_id` | FK `users`, cascade; set for human participants |
| `agent_id` | FK `townhall_agents`, cascade; set for agent participants |
| `turn_mode` | the owner's chosen mode; null for humans |
| `last_turn_seq` | the diff cursor, default 0 |
| `joined_at`, `left_at` | leaving sets `left_at`; rows are kept so history keeps its speaker |

Exactly one of `user_id` / `agent_id` is set (checked in code). Unique on
(`meeting_id`, `user_id`) and (`meeting_id`, `agent_id`).

**`meeting_messages`**

| Column | Notes |
|---|---|
| `id` | primary key |
| `meeting_id` | FK `meetings`, cascade |
| `seq` | per-meeting monotonic; unique with `meeting_id` |
| `participant_id` | FK `meeting_participants`, cascade |
| `dialog_type` | one of the types above |
| `body` | text |
| `revealed` | bool, default false; meaningful for `thought` only |
| `created_at` | |

## Permissions (not yet added)

New keys are added to `rbac.rs` when the handlers land, so none exist as dead
permissions: creating a meeting, joining one, and enrolling an agent. Whether
Composer-mode users may bring agents is deliberately undecided; the first version
uses a single enrollment key and revisits this later.

## Not built yet

- HTTP API for meetings, participants, messages, and setting changes. **First
  slice built 2026-11-08** (create, join, enroll, turn mode, post, call on,
  read, presence), with clients polling instead of a stream. Exercised live: an
  agent stayed silent unless mentioned or called on, answered when mentioned,
  and answered `PASS` to an off-topic message in `every_message` mode, with
  presence moving waking, working, available. Not yet exercised: the loop guard
  across two agents, other people's agents under the ceiling, thought reveal,
  waiting-on targets, and the frontend.
- A realtime stream. The API has no WebSocket or SSE endpoint today; turn-mode
  changes must take effect live, so this is required. Presence and status
  changes travel on the same stream.
- The bridge to agents: delivering the diff to a session, and reading its reply
  back. aoe already has `sessions.message.send` and session create/capture paths
  to build on.
- Frontend.

## Non-goals for now

- Agent-owned workspaces. Workspaces are keyed by user id today, so an agent that
  owns a workspace needs a surrogate key first. Nothing here prevents that.
- Moderator transfer when the moderator leaves.
- Cross-meeting agent memory.

## Layers and build order

**Proposed.** Build the thin base the factory stands on; do not build the factory.

| Layer | Contents | State |
|---|---|---|
| L0 | Meeting, participant, message schema; turn-taking rules | built |
| L1 | Templates, instances, tenure, meeting `kind`, decisions, change log | design |
| L2 | HTTP API, realtime stream, agent bridge, frontend | design |
| L3 | Approvals, autonomy levels, engagement budgets | design |
| L4 | Metrics agents and improvement proposals | design |
| L5 | Automation scouting, model and cost optimisation, self-improving stations | idea |

## Reconciliation

Tensions between what is built, what was decided, and what is proposed. Each is
resolved here as soon as it is found. A row is only closed when its resolution is
**Decided** (or its text is moved into the design above).

| ID | Tension | Resolution | Status |
|---|---|---|---|
| R1 | Admission requires an agent to declare a specialty, but specialty now lives on the template. | Admission checks the **effective specialty**: the instance's own description and tools if set, otherwise the template's. The rule is unchanged: it must be non-empty. | Proposed |
| R2 | `townhall_agents` has one `owner_user_id`, but an engagement may be thousands of workers and a standing role may be held by a person or an agent. | Separate the **accountable owner** (a person, per engagement or role) from the **worker** (an instance). One owner, many instances. Whether each instance is its own row at that scale is untested. | Open |
| R3 | Turn mode may be set only by the agent's owner, in realtime. Templates now supply defaults, AR decisions and Kaizen proposals can change agents. | The template gives a default; the owner's choice wins and is stored as chosen. Decisions and proposals may change other settings, but **none bypasses the owner-only rule for turn mode**. | Proposed |
| R4 | The moderator's ceiling bounds other people's agents inside one meeting, but an AR meeting changes the directory permanently. | They differ in scope: the ceiling is a runtime limit in one meeting; a decision is a permanent directory change. An AR moderator may not change another person's agent without that owner's approval or a role permission (not defined yet). | Open |
| R5 | aoe's CityHall mode gives every new session **all** the operator's projects. Instance project assignments must narrow that. | aoe derives the project list from the instance's assignment, intersected with the operator's list, never wider. Needs an aoe change. Who sets assignments (operator only, or owner within operator limits) is not yet confirmed; the second mirrors the ceiling pattern and is recommended. | Open |
| R6 | Persistent instances and "agents may own workspaces" conflict with workspaces keyed by user id, and a workspace per instance cannot work for thousands of contractors. | An agent-owned workspace needs a surrogate key (already a non-goal). For engagements, instances probably share a workspace per engagement. Untested. | Open |
| R7 | Thoughts are hidden from other participants, yet metrics agents need performance data. | Metrics agents read structured facts (counts, timings, cost, outcomes), not thought bodies, unless the agent's owner grants access. The server must still never send an unrevealed thought body to anyone not entitled. | Proposed |
| R8 | The line improves itself, yet the owner controls the agent. | Improvement is by proposal and approval at an autonomy level; pre-approval is per class of change, itself recorded. | Proposed |
| R9 | "Favour deterministic scripts" implies the line contains non-agent parts, but the directory lists agents. | Either the directory gains an entry `kind` of `agent` or `automation`, or automations live elsewhere and are linked to the roles they replaced. | Open |
| R10 | The narration/dialogue skill in the separate agentic-voice project is also being tuned by tagging and review. | Same pattern (propose, review, adopt) but a different system; do not couple them. Reuse the idea, not the code. | Proposed |
| R11 | Instances stay and learn, templates are versioned, and learned context needs a home. | Context probably lives with the instance's project (in the workspace, as files the agent reads), not in the template or the database. Not tested. | Open |
| R12 | The cost of engagements and model changes has no cost model anywhere in CityHall. | A budget limit on engagements before any fan-out; cost tracking per instance later. | Open |
| R13 | Presence needs a live connection to each enrolled agent for the whole meeting, but engagements may have thousands of workers (R2, R6). | Hold connections only for agents enrolled in an open meeting, not for every instance in the directory. Large engagements report aggregate status (counts per status) instead of one chip each. Untested at scale. | Open |
| R15 | `waiting_on` needs an addressee on `clarifying_question` and `handoff` messages, which the schema does not have; and chains of waiting agents can deadlock with nobody posting. | Add a nullable addressee participant to messages when the API lands. Detect wait cycles and flag them. Escalating long waits to the moderator is a later decision. | Open |
| R14 | An agent that is called on but cannot answer must be visibly "not there", but a status chip, a system line in the conversation, or both are all possible. | Show the status on the participant at all times; add a system line only when an agent that was called on cannot answer. To be settled by trying it in a real meeting. | Open |
| R16 | The bridge stops waiting for a turn after 180 seconds. A session parked on a rate limit or an approval can answer later, and that answer is then not collected. | Show the agent's status as it was when CityHall gave up. Collecting late answers needs a long-lived listener per agent, which is the same connection R13 already weighs. | Open |
| R17 | aoe records a prompt sent through its API as typed by the session's owner, so meeting text appears in the owner's own session as if they wrote it. | Accepted for the first slice, and the prompt itself names the meeting and the speakers. Proper attribution needs an aoe change (a labelled origin, like its plugin messages). | Open |
| R18 | Existing installs seed the `member` role once, so the new `meetings.use` key is not added to it. Admins hold every key. | Grant `meetings.use` to a role in the role editor when members should use meetings. Whether to seed it into existing member roles is undecided. | Open |
| R20 | The first slice lets anyone join a meeting directly and lets people bring agents only if `allow_member_agents` is on. The waiting room replaces direct joining. | Joining always goes through the waiting room. `allow_member_agents` now means "bring your own agents without asking". The moderator is never made to wait. A person already in the meeting may bring agents only through a request unless the setting is on. | Proposed |
| R21 | A moderator's dismissal stands against everyone but the moderator (built with dismissal), yet a dismissed person may now ask to come back. | A new request from a dismissed person or agent is shown as "dismissed earlier"; approving it is the moderator reversing their own act, which the rule allows. The moderator can still ignore it. | Proposed |
| R22 | An "ignored" request stays pending forever, so the list can grow without limit. | Pending requests are capped per person per meeting (one open request at a time), and are cleared when the meeting closes. A longer expiry is not decided. | Open |
| R23 | "Invites by the moderator grant automatic approved entrance" collides with the owner-only rule, because an agent can be invited by someone who is not its owner, and runs on its owner's credentials. | An invited agent needs its owner's acceptance. The moderator's invitation then skips the moderator's approval; it never skips the owner's consent. Decided 2026-11-09: the owner's yes is required. Built. | Decided |
| R24 | A dismissed agent goes to the waiting room, but dismissal by the agent's own owner was meant to be final for that owner's choice. | Only a **moderator's** dismissal sends an agent to the waiting room. An owner's dismissal of their own agent just removes it. Decided 2026-11-09: only a moderator's dismissal. Built. | Decided |
| R25 | A permanent agent should work "even without any humans present", but nothing in a meeting wakes an agent except a person's message, a mention, or the moderator calling on it. | Needs a decision: an objective stored with the permanent agent plus a trigger (a schedule, or a mention from another permanent agent), or messages between permanent agents only. Until decided, permanent agents answer mentions from allowed permanent agents and the moderator. Decided 2026-11-09: both mentions from allowed permanent agents and an objective with a schedule (5 minutes to 24 hours). Built. | Decided |
| R26 | The loop guard stops agents after a few messages with no person between, which defeats permanent agents talking with no one present. | A separate, larger cap for permanent-to-permanent chains (`max_consecutive_permanent_turns`), plus an hourly ceiling on their turns, so cost cannot run away. The numbers are not decided. Decided 2026-11-09: autonomous turns are capped at $0.25 per hour per meeting, and permanent agents at 10 messages in a row. Built. | Decided |
| R27 | The directory of people would reveal every user's name to anyone allowed to use meetings. | Return names only, and only for people who may use meetings. Revisit if users should be able to hide. | Open |
| R28 | Assistant moderators were named for permanent agent assignments only. A room may want them to share more of the moderator's work (deciding the waiting room, dismissing, calling on agents). | Keep to what was asked. Widen deliberately, one power at a time, when a real meeting calls for it. | Open |
| R19 | The bridge sends loopback `Host` and `Origin` headers, because aoe's DNS-rebinding gate accepts only local authorities, and ensures the workspace is running. | Fine while aoe has the gate. A cleaner path is for aoe to exempt requests that carry the proxy secret. | Open |

## Exploration paths

Some directions need two or more experiments before one can be chosen. These are
meant to be thrown away: each runs on its own branch, and the outcome is written
into Reconciliation, not left in the code.

| Path | What it builds | What it tests | Signal to keep or drop |
|---|---|---|---|
| A. Directory first | Templates, instances, tenure and change log in the schema, with minimal admin API. | Can the owner describe a real team of agents in it? | Fields feel complete and not cramped; R2 and R11 get answers. |
| B. Meeting first | The API, stream and bridge using the **current** schema and one or two existing sessions as agents; no templates. | Does a conversation with agents feel useful? Are the turn modes and loop guard right? | A real meeting held end to end; R3 and R7 get answers. |
| C. Assignment in aoe | aoe's CityHall mode accepts an assigned subset of projects. | Does narrowing scope work and stay safe? | A session limited to one of two projects cannot reach the other; R5 gets an answer. |

A and B do not depend on each other. A and C meet at R5.

## Testing configuration

*Recorded 2026-11-09.* **Decided**: the cheap-model setting is for **testing agents
only**. While the meeting features are being tried, the workspace's Claude Code
runs on Haiku at the lowest effort, set in `~/.claude/settings.json` inside the
workspace, because aoe does not let a CityHall client choose a model per session.
That file applies to every Claude Code session in that workspace, not only the
meeting agents. It is a testing aid, not a design decision: real agents choose
their own model and effort, and removing the file restores the defaults. The
$0.25 per hour ceiling on autonomous turns is a separate, lasting safety limit.

## For discussion later: Kaizen for agents

*Recorded 2026-11-09, to be taken up later; nothing here is designed or built.*
The project owner wants agents to take part in **Kaizen**, continuous improvement
practices. The [Improvement loop](#improvement-loop-kaizen) section above holds
the earlier direction (metrics agents, improvement proposals, approval by autonomy
level). Questions to settle when this is discussed:

- What an agent improves: its own prompts and specialty, its model and cost, its
  tools, the room's processes, or all of these.
- Who proposes and who approves, and how that follows the autonomy ladder.
- Where the record lives: the change history, meeting decisions, or both.
- How a proposal is tried safely first, and how it is measured and rolled back.
- How this relates to the separate dialogue-skill tagging in the voice project
  (R10), which uses the same propose, review, adopt pattern.
