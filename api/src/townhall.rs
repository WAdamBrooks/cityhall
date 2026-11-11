//! Town hall meeting rules that do not need a database: turn modes and how the
//! moderator's ceiling clamps them, the dialogue-type vocabulary, the loop guard,
//! and agent admission. See `docs/townhall.md`.

/// When an agent may speak. Ordered from least to most privileged, so `min`
/// clamps one to a ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TurnMode {
    /// Only when the moderator calls on it.
    ModeratorCalled,
    /// When anyone @mentions it (and when the moderator calls on it).
    MentionOnly,
    /// On any new message, if its specialty is triggered.
    EveryMessage,
}

impl TurnMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TurnMode::ModeratorCalled => "moderator_called",
            TurnMode::MentionOnly => "mention_only",
            TurnMode::EveryMessage => "every_message",
        }
    }

    pub fn parse(s: &str) -> Option<TurnMode> {
        match s {
            "moderator_called" => Some(TurnMode::ModeratorCalled),
            "mention_only" => Some(TurnMode::MentionOnly),
            "every_message" => Some(TurnMode::EveryMessage),
            _ => None,
        }
    }
}

/// The turn mode an agent actually runs with. The moderator's own agents run
/// with what their owner chose; everyone else's are clamped to the meeting's
/// ceiling. The owner's stored choice is never changed, so raising the ceiling
/// again restores it.
pub fn effective_mode(
    owner_mode: TurnMode,
    ceiling: TurnMode,
    owned_by_moderator: bool,
) -> TurnMode {
    if owned_by_moderator {
        owner_mode
    } else {
        owner_mode.min(ceiling)
    }
}

/// The kinds of message an agent or person can post.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogType {
    /// A conversational contribution to the topic.
    Contribution,
    /// What the agent is thinking; hidden behind dots until revealed.
    Thought,
    OfferHelp,
    ProgressReport,
    /// Proposed: a question the agent needs answered first.
    ClarifyingQuestion,
    /// Proposed: "outside my specialty; X is better placed".
    Handoff,
}

impl DialogType {
    pub fn as_str(self) -> &'static str {
        match self {
            DialogType::Contribution => "contribution",
            DialogType::Thought => "thought",
            DialogType::OfferHelp => "offer_help",
            DialogType::ProgressReport => "progress_report",
            DialogType::ClarifyingQuestion => "clarifying_question",
            DialogType::Handoff => "handoff",
        }
    }

    pub fn parse(s: &str) -> Option<DialogType> {
        match s {
            "contribution" => Some(DialogType::Contribution),
            "thought" => Some(DialogType::Thought),
            "offer_help" => Some(DialogType::OfferHelp),
            "progress_report" => Some(DialogType::ProgressReport),
            "clarifying_question" => Some(DialogType::ClarifyingQuestion),
            "handoff" => Some(DialogType::Handoff),
            _ => None,
        }
    }

    /// Whether a message of this type hides its body from other participants
    /// until it is revealed.
    pub fn hidden_until_revealed(self) -> bool {
        self == DialogType::Thought
    }
}

/// The loop guard. `recent_is_agent` lists the most recent messages' authors,
/// newest last (`true` for an agent). An agent may post unless the last `cap`
/// messages were all from agents.
pub fn agent_turn_allowed(recent_is_agent: &[bool], cap: usize) -> bool {
    if cap == 0 {
        return false;
    }
    let trailing_agents = recent_is_agent.iter().rev().take_while(|a| **a).count();
    trailing_agents < cap
}

/// Who may dismiss a participant: the moderator may dismiss anyone, and an
/// agent's owner may dismiss their own agent. No one else may.
pub fn may_dismiss(caller_is_moderator: bool, caller_owns_agent: bool) -> bool {
    caller_is_moderator || caller_owns_agent
}

/// Who may make and adjust permanent agent assignments: the moderator and the
/// assistant moderators the moderator has named. Everyone else works through them.
pub fn may_assign_permanent(is_moderator: bool, is_assistant: bool) -> bool {
    is_moderator || is_assistant
}

/// Whether a viewer sees an agent in the room list. Its owner always does, and so
/// does the moderator, who has to be able to manage the room. A permanent agent
/// is part of the room itself, so everyone does. Any other agent shows to other
/// people only when its owner has made it visible.
pub fn agent_listed_for(
    viewer_owns: bool,
    viewer_is_moderator: bool,
    permanent: bool,
    visible_to_others: bool,
) -> bool {
    viewer_owns || viewer_is_moderator || permanent || visible_to_others
}

/// Whether `actor` may bring back an agent whose participation ended. What the
/// moderator ended, only the moderator may undo; an owner's own dismissal of
/// their agent is theirs to undo.
pub fn may_reenroll(left_by: Option<i32>, moderator: i32, actor: i32) -> bool {
    !(left_by == Some(moderator) && actor != moderator)
}

/// What a waiting-room or invitation line shows its viewers. A person's
/// invitation reads "invited" until they are present, and "waiting" once they are
/// and the moderator has yet to decide. A moderator's own invitation never waits
/// on the moderator.
pub fn line_display(
    status: &str,
    kind: &str,
    invited_by_moderator: bool,
    is_agent: bool,
    present: bool,
) -> &'static str {
    match status {
        "approved" => "in the room",
        "denied" => "denied",
        "declined" => "declined",
        "withdrawn" => "withdrawn",
        "invited" => "invited",
        // Waiting for the moderator.
        _ if kind == "invite" && !is_agent && !invited_by_moderator && !present => "invited",
        _ => "waiting",
    }
}

/// The names `@`-mentioned in a message, in order and without repeats. A name is
/// letters, digits, `-` and `_`, matched without regard to case by the caller.
pub fn mentioned_names(body: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut chars = body.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '@' {
            continue;
        }
        // An `@` inside a word (an email address) is not a mention.
        if body[..i]
            .chars()
            .next_back()
            .is_some_and(|p| p.is_alphanumeric())
        {
            continue;
        }
        let mut name = String::new();
        while let Some(&(_, n)) = chars.peek() {
            if n.is_alphanumeric() || n == '-' || n == '_' {
                name.push(n);
                chars.next();
            } else {
                break;
            }
        }
        if !name.is_empty() && !names.iter().any(|x| x.eq_ignore_ascii_case(&name)) {
            names.push(name);
        }
    }
    names
}

/// Which of the agents a permanent agent named it may actually reach: only those
/// on its list. The rest are dropped, whatever the message said.
pub fn reachable(named: &[i32], allowed: &[i32]) -> Vec<i32> {
    named
        .iter()
        .copied()
        .filter(|n| allowed.contains(n))
        .collect()
}

/// Whether the autonomous spend of the last hour leaves room for another turn.
pub fn within_hourly_budget(spent_cents_last_hour: f64, limit_cents: i32) -> bool {
    spent_cents_last_hour < f64::from(limit_cents)
}

/// Why an agent was not admitted.
#[derive(Debug, PartialEq, Eq)]
pub enum AdmissionError {
    EmptyDescription,
    NoTools,
}

/// An agent is admitted only with a specialty: a non-empty description and at
/// least one tool. `tools_json` is the stored JSON array.
pub fn check_admission(description: &str, tools_json: &str) -> Result<(), AdmissionError> {
    if description.trim().is_empty() {
        return Err(AdmissionError::EmptyDescription);
    }
    let tools: Vec<String> = serde_json::from_str(tools_json).unwrap_or_default();
    if tools.iter().all(|t| t.trim().is_empty()) {
        return Err(AdmissionError::NoTools);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_peoples_agents_show_only_when_made_visible() {
        // (owns, moderator, permanent, visible)
        assert!(agent_listed_for(true, false, false, false));
        assert!(agent_listed_for(false, true, false, false));
        assert!(agent_listed_for(false, false, true, false));
        assert!(agent_listed_for(false, false, false, true));
        assert!(!agent_listed_for(false, false, false, false));
    }

    #[test]
    fn an_invitation_shows_invited_until_the_person_is_present() {
        // A person invited by someone other than the moderator.
        assert_eq!(
            line_display("pending", "invite", false, false, false),
            "invited"
        );
        assert_eq!(
            line_display("pending", "invite", false, false, true),
            "waiting"
        );
        // An agent has no presence: once its owner accepts, it waits.
        assert_eq!(
            line_display("pending", "invite", false, true, false),
            "waiting"
        );
        assert_eq!(
            line_display("invited", "invite", false, true, true),
            "invited"
        );
        // A person's own ask is simply waiting.
        assert_eq!(
            line_display("pending", "ask", false, false, false),
            "waiting"
        );
        // Decided lines read as their verdict.
        assert_eq!(
            line_display("approved", "invite", true, false, false),
            "in the room"
        );
        assert_eq!(
            line_display("declined", "invite", false, false, true),
            "declined"
        );
    }

    #[test]
    fn mentions_are_names_after_an_at_sign() {
        assert_eq!(
            mentioned_names("@Rev and @scribe-2, please"),
            ["Rev", "scribe-2"]
        );
        assert_eq!(mentioned_names("@Rev @rev again"), ["Rev"]);
        assert!(mentioned_names("mail me at a@b.com or just @ alone").is_empty());
        assert_eq!(mentioned_names("(@Rev)"), ["Rev"]);
    }

    #[test]
    fn a_permanent_agent_reaches_only_those_on_its_list() {
        assert_eq!(reachable(&[1, 2, 3], &[2, 3, 9]), [2, 3]);
        assert!(reachable(&[1], &[]).is_empty());
    }

    #[test]
    fn the_hourly_budget_stops_further_turns_once_spent() {
        assert!(within_hourly_budget(24.9, 25));
        assert!(!within_hourly_budget(25.0, 25));
        assert!(!within_hourly_budget(0.0, 0));
    }

    #[test]
    fn only_the_moderator_or_an_agents_owner_may_dismiss() {
        assert!(may_dismiss(true, false));
        assert!(may_dismiss(false, true));
        assert!(may_dismiss(true, true));
        assert!(!may_dismiss(false, false));
    }

    #[test]
    fn only_the_moderator_and_assistants_assign_permanent_agents() {
        assert!(may_assign_permanent(true, false));
        assert!(may_assign_permanent(false, true));
        assert!(!may_assign_permanent(false, false));
    }

    #[test]
    fn what_the_moderator_ended_only_the_moderator_may_undo() {
        // Moderator is user 1, an owner is user 2.
        assert!(!may_reenroll(Some(1), 1, 2));
        assert!(may_reenroll(Some(1), 1, 1));
        // An owner's own dismissal is theirs to undo.
        assert!(may_reenroll(Some(2), 1, 2));
        assert!(may_reenroll(None, 1, 2));
    }

    #[test]
    fn modes_are_ordered_least_to_most_privileged() {
        assert!(TurnMode::ModeratorCalled < TurnMode::MentionOnly);
        assert!(TurnMode::MentionOnly < TurnMode::EveryMessage);
    }

    #[test]
    fn turn_modes_round_trip_through_their_stored_strings() {
        for m in [
            TurnMode::ModeratorCalled,
            TurnMode::MentionOnly,
            TurnMode::EveryMessage,
        ] {
            assert_eq!(TurnMode::parse(m.as_str()), Some(m));
        }
        assert_eq!(TurnMode::parse("always"), None);
    }

    #[test]
    fn ceiling_clamps_other_peoples_agents() {
        use TurnMode::*;
        assert_eq!(
            effective_mode(EveryMessage, MentionOnly, false),
            MentionOnly
        );
        assert_eq!(
            effective_mode(EveryMessage, ModeratorCalled, false),
            ModeratorCalled
        );
        // A choice at or below the ceiling is left alone.
        assert_eq!(
            effective_mode(ModeratorCalled, MentionOnly, false),
            ModeratorCalled
        );
        assert_eq!(effective_mode(MentionOnly, MentionOnly, false), MentionOnly);
    }

    #[test]
    fn ceiling_does_not_apply_to_the_moderators_own_agents() {
        use TurnMode::*;
        assert_eq!(
            effective_mode(EveryMessage, ModeratorCalled, true),
            EveryMessage
        );
    }

    #[test]
    fn raising_the_ceiling_restores_the_owners_choice() {
        use TurnMode::*;
        let chosen = EveryMessage;
        assert_eq!(
            effective_mode(chosen, ModeratorCalled, false),
            ModeratorCalled
        );
        assert_eq!(effective_mode(chosen, EveryMessage, false), EveryMessage);
    }

    #[test]
    fn dialog_types_round_trip_and_only_thoughts_hide() {
        for t in [
            DialogType::Contribution,
            DialogType::Thought,
            DialogType::OfferHelp,
            DialogType::ProgressReport,
            DialogType::ClarifyingQuestion,
            DialogType::Handoff,
        ] {
            assert_eq!(DialogType::parse(t.as_str()), Some(t));
            assert_eq!(t.hidden_until_revealed(), t == DialogType::Thought);
        }
        assert_eq!(DialogType::parse("rant"), None);
    }

    #[test]
    fn loop_guard_blocks_after_cap_agent_messages_in_a_row() {
        // Human, then three agents in a row, cap 3: blocked.
        assert!(!agent_turn_allowed(&[false, true, true, true], 3));
        // Two agents in a row: still allowed.
        assert!(agent_turn_allowed(&[false, true, true], 3));
        // A human message resets the run.
        assert!(agent_turn_allowed(&[true, true, true, false], 3));
        // Empty meeting.
        assert!(agent_turn_allowed(&[], 3));
        // A zero cap means agents never post on their own.
        assert!(!agent_turn_allowed(&[false], 0));
    }

    #[test]
    fn admission_needs_a_description_and_a_tool() {
        assert_eq!(check_admission("reviews SQL", r#"["psql"]"#), Ok(()));
        assert_eq!(
            check_admission("  ", r#"["psql"]"#),
            Err(AdmissionError::EmptyDescription)
        );
        assert_eq!(
            check_admission("reviews SQL", "[]"),
            Err(AdmissionError::NoTools)
        );
        assert_eq!(
            check_admission("reviews SQL", r#"[" "]"#),
            Err(AdmissionError::NoTools)
        );
        // Malformed stored tools count as none.
        assert_eq!(
            check_admission("reviews SQL", "not json"),
            Err(AdmissionError::NoTools)
        );
    }
}
