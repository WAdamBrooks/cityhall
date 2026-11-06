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

Unique on (`owner_user_id`, `session_id`).

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

- HTTP API for meetings, participants, messages, and setting changes.
- A realtime stream. The API has no WebSocket or SSE endpoint today; turn-mode
  changes must take effect live, so this is required.
- The bridge to agents: delivering the diff to a session, and reading its reply
  back. aoe already has `sessions.message.send` and session create/capture paths
  to build on.
- Frontend.

## Non-goals for now

- Agent-owned workspaces. Workspaces are keyed by user id today, so an agent that
  owns a workspace needs a surrogate key first. Nothing here prevents that.
- Moderator transfer when the moderator leaves.
- Cross-meeting agent memory.
