#![allow(clippy::unwrap_used)]
use super::*;

fn launcher(choice: AgentChoice) -> Launcher {
    Launcher {
        program: PathBuf::from("/bin/sheprd-acp"),
        choice,
    }
}

#[test]
fn a_program_that_offers_the_choice_starts_chats_that_can_switch() {
    assert_eq!(
        launcher(AgentChoice::Open).argv(Agent::Claude),
        ["/bin/sheprd-acp", "--choose", "claude"]
    );
    assert_eq!(
        launcher(AgentChoice::Fixed).argv(Agent::Codex),
        ["/bin/sheprd-acp", "codex"]
    );
}

#[test]
fn a_program_is_asked_again_only_once_it_changes() {
    let asked = Asked::default();
    let program = Path::new("/bin/sheprd-acp");
    let then = Some(SystemTime::UNIX_EPOCH);
    assert_eq!(asked.get(program, then), None);
    asked.set(program.to_owned(), then, AgentChoice::Open);
    assert_eq!(asked.get(program, then), Some(AgentChoice::Open));
    assert_eq!(asked.get(program, Some(SystemTime::now())), None);
    assert_eq!(asked.get(Path::new("/other/sheprd-acp"), then), None);
}
