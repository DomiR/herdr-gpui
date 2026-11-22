//! The `continued` cases: a terminal agent's compacted conversation moved
//! into this chat. `continued` opens folded above the last prompt the agent
//! had, `continued-open` shows the earlier turns and the compaction summary,
//! and `continued-before` is how the same log read before compaction
//! became a notice: its summary as the person's prompt, with no fold.

use sheprd_chat_protocol::{
    Entry, EntryId, EntryKind, NoticeLevel, SessionInfo, Tool, ToolContent, ToolKind, ToolStatus,
    Transcript,
};

/// The notice under the moved conversation.
pub(super) const DIVIDER: EntryId = EntryId(9);
/// The compaction notice, whose details hold the summary.
pub(super) const COMPACTED: EntryId = EntryId(3);

const SUMMARY: &str = "This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation.\n\n<system-reminder>\n<tools-used-before-summary>Bash, Edit, Read</tools-used-before-summary>\n</system-reminder>\nSummary:\n1. Primary Request and Intent:\n   - Make the new window layout the default, keeping the old one behind a setting.\n2. Key Technical Concepts:\n   - The layout script and the Rust view it hosts.\n3. Pending Tasks:\n   - Run the layout tests and open the pull request."; // i18n: data

fn user(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            files: 0,
            note: None,
            text: text.into(),
        },
    }
}

fn agent(id: u64, text: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Agent { text: text.into() },
    }
}

fn notice(id: EntryId, text: &str, details: Option<&str>) -> Entry {
    Entry {
        id,
        kind: EntryKind::Notice {
            answers: Vec::new(),
            details: details.map(str::to_owned),
            level: NoticeLevel::Info,
            text: text.into(),
        },
    }
}

fn tool(id: u64) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: "layout-tests".into(),
            content: vec![ToolContent::Text {
                text: "cargo test -p layout\n\ntest result: ok. 42 passed".into(), // i18n: data
            }],
            kind: ToolKind::Execute,
            locations: Vec::new(),
            parent: None,
            started_ms: None,
            status: ToolStatus::Completed,
            title: "cargo test -p layout".into(), // i18n: data
        }),
    }
}

pub(super) fn apply(case: &str, transcript: &mut Transcript) {
    transcript.agent = "Claude".into(); // i18n: data
    let compaction = if case == "continued-before" {
        user(COMPACTED.0, SUMMARY)
    } else {
        notice(COMPACTED, "Context compacted", Some(SUMMARY)) // i18n: data
    };
    transcript.entries = vec![
        user(1, "Make the new window layout the default."), // i18n: data
        agent(
            2,
            // i18n: data
            "Started: the new layout now opens by default; the old one stays behind a setting.",
        ),
        compaction,
        user(4, "Run the layout tests, then open the pull request."), // i18n: data
        tool(5),
        agent(
            6,
            // i18n: data
            "All 42 layout tests pass. The pull request is open as a draft.",
        ),
    ];
    if case == "continued-before" {
        return;
    }
    transcript.entries.push(notice(
        DIVIDER,
        "Continued from Claude's earlier session", // i18n: data
        None,
    ));
    transcript.session = Some(SessionInfo {
        continued: Some(DIVIDER),
        cwd: None,
        id: "synthetic-preview-session".into(),
        title: None,
    });
}
