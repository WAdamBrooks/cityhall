//! The bridge between a town hall meeting and an agent's aoe session.
//!
//! CityHall reaches the session through the same authenticated hop the workspace
//! proxy uses: it sends a prompt with `POST /api/sessions/{id}/acp/prompt`, then
//! polls `GET /api/sessions/{id}/acp/replay?since=N` for the events that follow
//! until the turn stops. Polling is a deliberate first step: it needs no
//! WebSocket client, and the replay cursor is the same one the live stream uses.
//! See `docs/townhall.md`, "What an agent sees" and "Presence and status".

use std::time::Duration;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;
use crate::state::AppState;
use crate::townhall::DialogType;

/// aoe checks the `Host` and `Origin` of every request (DNS-rebinding gate) and
/// accepts loopback authorities, which is also what a browser behind the proxy
/// presents.
const LOCAL_HOST: &str = "127.0.0.1";
const LOCAL_ORIGIN: &str = "http://127.0.0.1";

/// Most replay pages read in one poll, so a runaway session cannot pin a turn.
const MAX_REPLAY_PAGES: usize = 20;
const REPLAY_PAGE_LIMIT: u32 = 200;

/// A client for one user's workspace.
pub struct WorkspaceClient {
    http: reqwest::Client,
    base: String,
    token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub acp_capable: bool,
    #[serde(default)]
    pub view: String,
}

#[derive(Debug, Deserialize)]
struct SessionList {
    sessions: Vec<SessionInfo>,
}

/// What aoe says when it accepts a prompt. `disposition` is `sent` when the
/// session was idle; anything else means the prompt waits behind a running turn.
#[derive(Debug, Deserialize)]
pub struct PromptAck {
    pub disposition: String,
    pub seq: u64,
}

#[derive(Debug, Deserialize)]
pub struct ReplayFrame {
    pub seq: u64,
    pub event: Value,
}

#[derive(Debug, Default, Deserialize)]
struct ReplayPage {
    #[serde(default)]
    frames: Vec<ReplayFrame>,
    #[serde(default)]
    next_cursor: Option<u64>,
    #[serde(default)]
    has_more: bool,
}

impl WorkspaceClient {
    /// Resolve the workspace address and its proxy secret. Starts the workspace
    /// if it is stopped, exactly as a proxied request would.
    pub async fn for_user(state: &AppState, user_id: i32) -> Result<Self, AppError> {
        let addr = crate::workspaces::ensure_started(state, user_id).await?;
        let token = crate::proxy::proxy_token_for(state, user_id).await?;
        Ok(Self {
            http: state.proxy_client.clone(),
            base: format!("http://{addr}"),
            token,
        })
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<String>,
    ) -> Result<Vec<u8>, AppError> {
        let mut req = self
            .http
            .request(method, format!("{}{path}", self.base))
            .timeout(Duration::from_secs(30))
            .header("host", LOCAL_HOST)
            .header("origin", LOCAL_ORIGIN)
            .header("x-forwarded-proto", "http")
            .header("content-type", "application/json");
        if let Some(token) = &self.token {
            req = req.header(crate::proxy::PROXY_TOKEN_HEADER, token);
        }
        if let Some(body) = body {
            req = req.body(body);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| AppError::WorkspaceUnavailable(format!("workspace unreachable: {e}")))?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::WorkspaceUnavailable(format!("workspace read failed: {e}")))?;
        if !status.is_success() {
            return Err(AppError::WorkspaceUnavailable(format!(
                "workspace answered {status}"
            )));
        }
        Ok(bytes.to_vec())
    }

    /// The sessions in this workspace.
    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>, AppError> {
        let bytes = self.call(Method::GET, "/api/sessions", None).await?;
        let list: SessionList = serde_json::from_slice(&bytes)
            .map_err(|_| AppError::Internal("unexpected session list from workspace"))?;
        Ok(list.sessions)
    }

    /// Start a new structured Claude Code session in a scratch directory and
    /// return its id. This is what makes an agent creatable from CityHall alone.
    pub async fn create_session(&self, title: &str) -> Result<String, AppError> {
        let body = serde_json::json!({
            "title": title,
            "path": "",
            "tool": "claude",
            "scratch": true,
            "view": "structured",
            "trust_hooks": true,
        })
        .to_string();
        let bytes = self
            .call(Method::POST, "/api/sessions?wait=ready", Some(body))
            .await?;
        let created: Value = serde_json::from_slice(&bytes)
            .map_err(|_| AppError::Internal("unexpected answer creating a session"))?;
        created
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(AppError::Internal("the workspace returned no session id"))
    }

    /// Hand a prompt to a structured session.
    pub async fn send_prompt(&self, session_id: &str, text: &str) -> Result<PromptAck, AppError> {
        let body = serde_json::json!({ "text": text }).to_string();
        let bytes = self
            .call(
                Method::POST,
                &format!("/api/sessions/{session_id}/acp/prompt"),
                Some(body),
            )
            .await?;
        serde_json::from_slice(&bytes)
            .map_err(|_| AppError::Internal("unexpected prompt answer from workspace"))
    }

    /// Every event after `since` (exclusive), following paging to the end.
    pub async fn events_since(
        &self,
        session_id: &str,
        since: u64,
    ) -> Result<Vec<ReplayFrame>, AppError> {
        let mut cursor = since;
        let mut frames = Vec::new();
        for _ in 0..MAX_REPLAY_PAGES {
            let path = format!(
                "/api/sessions/{session_id}/acp/replay?since={cursor}&limit={REPLAY_PAGE_LIMIT}"
            );
            let bytes = self.call(Method::GET, &path, None).await?;
            let page: ReplayPage = serde_json::from_slice(&bytes)
                .map_err(|_| AppError::Internal("unexpected replay from workspace"))?;
            let more = page.has_more;
            let next = page.next_cursor;
            frames.extend(page.frames);
            match (more, next) {
                (true, Some(next)) if next > cursor => cursor = next,
                _ => break,
            }
        }
        Ok(frames)
    }
}

/// What an agent is doing, as shown next to its name. See `docs/townhall.md`,
/// "Presence and status".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    Available,
    Thinking,
    Working,
    WaitingOn,
    RateLimited,
    Waking,
    // `needs_login` (see the design document) is added once aoe's auth events are
    // wired in; nothing produces it yet.
    Unreachable,
}

/// Why an agent is waiting on someone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    Approval,
    Answer,
}

/// How a turn ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnEnd {
    /// aoe reported the turn stopped, with its reason (`prompt_complete` is a
    /// normal finish).
    Stopped(String),
    /// The session or its adapter failed.
    Failed(String),
}

/// The running picture of one turn, built up from replay events.
#[derive(Clone, Debug)]
pub struct TurnProgress {
    pub state: PresenceState,
    pub waiting: Option<WaitReason>,
    pub reply: String,
    pub ended: Option<TurnEnd>,
    pub last_seq: u64,
    /// The session's running total cost in US dollars, as last reported. aoe
    /// reports a total for the whole session, so a turn's cost is the change.
    pub cost_usd: Option<f64>,
    last_message_id: Option<String>,
}

impl TurnProgress {
    /// A turn whose prompt has been accepted; `after` is the seq to poll from.
    pub fn new(after: u64) -> Self {
        Self {
            state: PresenceState::Working,
            waiting: None,
            reply: String::new(),
            ended: None,
            last_seq: after,
            cost_usd: None,
            last_message_id: None,
        }
    }

    /// Fold new events in, in order.
    pub fn apply(&mut self, frames: &[ReplayFrame]) {
        for frame in frames {
            self.last_seq = self.last_seq.max(frame.seq);
            if self.ended.is_some() {
                continue;
            }
            let (kind, body) = match &frame.event {
                Value::String(name) => (name.as_str(), &Value::Null),
                Value::Object(map) => match map.iter().next() {
                    Some((name, body)) => (name.as_str(), body),
                    None => continue,
                },
                _ => continue,
            };
            match kind {
                "ThinkingStarted" | "AgentThoughtChunk" => {
                    self.state = PresenceState::Thinking;
                    self.waiting = None;
                }
                "ThinkingEnded"
                | "ToolCallStarted"
                | "ApprovalResolved"
                | "ElicitationResolved" => {
                    self.state = PresenceState::Working;
                    self.waiting = None;
                }
                "ApprovalRequested" => {
                    self.state = PresenceState::WaitingOn;
                    self.waiting = Some(WaitReason::Approval);
                }
                "ElicitationRequested" => {
                    self.state = PresenceState::WaitingOn;
                    self.waiting = Some(WaitReason::Answer);
                }
                "RateLimit" => self.state = PresenceState::RateLimited,
                "UsageUpdated" => {
                    if let Some(amount) = body.pointer("/usage/cost/amount").and_then(Value::as_f64)
                    {
                        self.cost_usd = Some(amount);
                    }
                }
                "AgentMessageChunk" => {
                    self.state = PresenceState::Working;
                    self.waiting = None;
                    let text = body.get("text").and_then(Value::as_str).unwrap_or("");
                    let id = body
                        .get("message_id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    // A new message id starts a new paragraph of the reply.
                    if id.is_some() && id != self.last_message_id && !self.reply.is_empty() {
                        self.reply.push_str("\n\n");
                    }
                    self.last_message_id = id;
                    self.reply.push_str(text);
                }
                "Stopped" => {
                    let reason = body
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("stopped");
                    self.ended = Some(TurnEnd::Stopped(reason.to_string()));
                }
                "PromptRuntimeError" | "AgentStartupError" => {
                    let message = body
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("the agent failed");
                    self.ended = Some(TurnEnd::Failed(message.to_string()));
                }
                _ => {}
            }
        }
    }
}

/// What an agent's reply means for the meeting.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    /// The agent chose to stay silent.
    Pass,
    Say {
        dialog_type: DialogType,
        body: String,
    },
}

/// The sentinel an agent answers with to stay silent.
pub const PASS: &str = "PASS";

/// Turn an agent's raw answer into a meeting message. A leading `[type]` tag
/// picks the dialogue type; without one the answer is a `contribution`.
pub fn parse_reply(raw: &str) -> Reply {
    let text = raw.trim();
    let bare = text.trim_matches(|c: char| !c.is_alphanumeric());
    if text.is_empty() || bare.eq_ignore_ascii_case(PASS) {
        return Reply::Pass;
    }
    if let Some(rest) = text.strip_prefix('[') {
        if let Some((tag, body)) = rest.split_once(']') {
            if let Some(dialog_type) = DialogType::parse(tag.trim()) {
                let body = body.trim();
                if body.is_empty() {
                    return Reply::Pass;
                }
                return Reply::Say {
                    dialog_type,
                    body: body.to_string(),
                };
            }
        }
    }
    Reply::Say {
        dialog_type: DialogType::Contribution,
        body: text.to_string(),
    }
}

/// One line of the conversation as an agent sees it.
pub struct Line<'a> {
    pub speaker: &'a str,
    pub is_agent: bool,
    pub body: &'a str,
}

/// The prompt an agent receives for its turn: the rules of the room, then the
/// conversation since its last turn.
/// What a turn is for, beyond the conversation itself.
pub struct TurnContext<'a> {
    pub called_on: bool,
    /// A permanent agent's standing objective, when it was woken for it.
    pub objective: Option<&'a str>,
    /// For a permanent agent: the names of the agents it may address. `None` for
    /// an ordinary agent, which may not address other agents directly.
    pub may_address: Option<&'a [String]>,
}

pub fn build_prompt(
    meeting_title: &str,
    agent_name: &str,
    context: &TurnContext,
    lines: &[Line],
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "You are {agent_name}, a participant in the meeting \"{meeting_title}\" with other \
         people and agents. Answer only as {agent_name}, within your specialty.\n\n\
         Reply with exactly one of:\n\
         - {PASS} (the single word, nothing else) if you have nothing worth adding right now.\n\
         - Your contribution, brief and to the point. Begin it with a tag line in square \
         brackets when it is not a plain contribution: [offer_help], [progress_report], \
         [clarifying_question] or [handoff].\n\n"
    ));
    if context.called_on {
        out.push_str("The moderator has called on you to speak.\n\n");
    }
    if let Some(objective) = context.objective {
        out.push_str(&format!(
            "You are a permanent resident of this meeting, working on this standing \
             objective, whether or not any person is present: {objective}\n\n"
        ));
    }
    if let Some(names) = context.may_address {
        if names.is_empty() {
            out.push_str("You may not address any other agent.\n\n");
        } else {
            let list = names
                .iter()
                .map(|n| format!("@{n}"))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "To prompt another agent, mention it by name, for example @Name. You may \
                 address only: {list}. Anyone else you mention will not hear you.\n\n"
            ));
        }
    }
    out.push_str("The conversation since your last turn:\n");
    for line in lines {
        let kind = if line.is_agent { "agent" } else { "person" };
        out.push_str(&format!("{} ({kind}): {}\n", line.speaker, line.body));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame(seq: u64, event: Value) -> ReplayFrame {
        ReplayFrame { seq, event }
    }

    #[test]
    fn a_finished_turn_collects_its_reply_and_ends_on_stopped() {
        let mut turn = TurnProgress::new(40);
        turn.apply(&[
            frame(41, json!({"UserPromptSent": {"text": "q"}})),
            frame(
                42,
                json!({"AgentMessageChunk": {"text": "2 +", "message_id": "m1"}}),
            ),
            frame(
                43,
                json!({"AgentMessageChunk": {"text": " 2 equals 4.", "message_id": "m1"}}),
            ),
            frame(44, json!({"Stopped": {"reason": "prompt_complete"}})),
        ]);
        assert_eq!(turn.reply, "2 + 2 equals 4.");
        assert_eq!(turn.ended, Some(TurnEnd::Stopped("prompt_complete".into())));
        assert_eq!(turn.last_seq, 44);
    }

    #[test]
    fn a_new_message_id_starts_a_new_paragraph() {
        let mut turn = TurnProgress::new(0);
        turn.apply(&[
            frame(
                1,
                json!({"AgentMessageChunk": {"text": "First.", "message_id": "a"}}),
            ),
            frame(
                2,
                json!({"AgentMessageChunk": {"text": "Second.", "message_id": "b"}}),
            ),
        ]);
        assert_eq!(turn.reply, "First.\n\nSecond.");
    }

    #[test]
    fn status_follows_the_events() {
        let mut turn = TurnProgress::new(0);
        turn.apply(&[frame(1, json!("ThinkingStarted"))]);
        assert_eq!(turn.state, PresenceState::Thinking);
        turn.apply(&[frame(2, json!({"ToolCallStarted": {}}))]);
        assert_eq!(turn.state, PresenceState::Working);
        turn.apply(&[frame(3, json!({"ApprovalRequested": {}}))]);
        assert_eq!(turn.state, PresenceState::WaitingOn);
        assert_eq!(turn.waiting, Some(WaitReason::Approval));
        turn.apply(&[frame(4, json!({"ApprovalResolved": {}}))]);
        assert_eq!(turn.state, PresenceState::Working);
        assert_eq!(turn.waiting, None);
        turn.apply(&[frame(5, json!({"RateLimit": {}}))]);
        assert_eq!(turn.state, PresenceState::RateLimited);
    }

    #[test]
    fn a_failure_ends_the_turn_and_later_events_are_ignored() {
        let mut turn = TurnProgress::new(0);
        turn.apply(&[
            frame(1, json!({"PromptRuntimeError": {"message": "boom"}})),
            frame(
                2,
                json!({"AgentMessageChunk": {"text": "late", "message_id": "x"}}),
            ),
        ]);
        assert_eq!(turn.ended, Some(TurnEnd::Failed("boom".into())));
        assert_eq!(turn.reply, "");
        assert_eq!(turn.last_seq, 2);
    }

    #[test]
    fn pass_in_any_dress_means_silence() {
        for raw in ["PASS", " pass ", "PASS.", "`PASS`", "", "   "] {
            assert_eq!(parse_reply(raw), Reply::Pass, "{raw:?}");
        }
        // Not silence: a sentence that merely contains the word.
        assert!(matches!(
            parse_reply("I will not PASS on this."),
            Reply::Say { .. }
        ));
    }

    #[test]
    fn a_leading_tag_picks_the_dialogue_type() {
        assert_eq!(
            parse_reply("[offer_help] I can review that."),
            Reply::Say {
                dialog_type: DialogType::OfferHelp,
                body: "I can review that.".into()
            }
        );
        assert_eq!(
            parse_reply("Plain words."),
            Reply::Say {
                dialog_type: DialogType::Contribution,
                body: "Plain words.".into()
            }
        );
        // An unknown tag is part of the text, not a type.
        assert_eq!(
            parse_reply("[note] hello"),
            Reply::Say {
                dialog_type: DialogType::Contribution,
                body: "[note] hello".into()
            }
        );
    }

    #[test]
    fn the_prompt_names_the_room_the_rules_and_the_conversation() {
        let prompt = build_prompt(
            "Zero",
            "Reviewer",
            &TurnContext {
                called_on: true,
                objective: None,
                may_address: None,
            },
            &[
                Line {
                    speaker: "Alice",
                    is_agent: false,
                    body: "hi",
                },
                Line {
                    speaker: "Bot",
                    is_agent: true,
                    body: "hello",
                },
            ],
        );
        assert!(prompt.contains("You are Reviewer"));
        assert!(prompt.contains("PASS"));
        assert!(prompt.contains("called on you"));
        assert!(prompt.contains("Alice (person): hi"));
        assert!(prompt.contains("Bot (agent): hello"));
        // An ordinary agent is told nothing about addressing other agents.
        assert!(!prompt.contains("address only"));
    }

    #[test]
    fn a_permanent_agent_is_told_its_objective_and_exactly_who_it_may_address() {
        let names = vec!["Scribe".to_string(), "Rev".to_string()];
        let prompt = build_prompt(
            "Zero",
            "Watcher",
            &TurnContext {
                called_on: false,
                objective: Some("keep the build green"),
                may_address: Some(&names),
            },
            &[],
        );
        assert!(prompt.contains("keep the build green"));
        assert!(prompt.contains("address only: @Scribe, @Rev"));

        let alone = vec![];
        let none = build_prompt(
            "Zero",
            "Watcher",
            &TurnContext {
                called_on: false,
                objective: None,
                may_address: Some(&alone),
            },
            &[],
        );
        assert!(none.contains("may not address any other agent"));
    }

    #[test]
    fn the_session_cost_is_read_from_usage_events() {
        let mut turn = TurnProgress::new(0);
        turn.apply(&[
            frame(
                1,
                json!({"UsageUpdated": {"usage": {"used": 5, "size": 9, "cost": null}}}),
            ),
            frame(
                2,
                json!({"UsageUpdated": {"usage": {"cost": {"amount": 0.0461, "currency": "USD"}}}}),
            ),
        ]);
        assert_eq!(turn.cost_usd, Some(0.0461));
    }
}
