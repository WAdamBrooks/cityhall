//! Town hall meeting rules that do not need a database: turn modes and how the
//! moderator's ceiling clamps them, the dialogue-type vocabulary, the loop guard,
//! and agent admission. See `docs/townhall.md`.
//!
//! Nothing here is wired to a handler yet, hence the `dead_code` allowance; the
//! handlers and the realtime stream land next.
#![allow(dead_code)]

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
