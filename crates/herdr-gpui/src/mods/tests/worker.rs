use super::*;
use crate::mods::worker::{Handle, Report};
use herdr_mods::{Agent, AgentStatus};

/// Waits for the worker's next tree, collecting everything else it reported.
fn next_tree(worker: &Handle, seen: &mut Vec<Report>) -> Node {
    eventually(|| {
        let report = worker.take();
        let tree = report.tree.clone();
        seen.push(report);
        tree
    })
}

fn agents(count: usize) -> Arc<State> {
    Arc::new(State {
        agents: (0..count)
            .map(|index| Agent {
                id: format!("p{index}"),
                host: "local".into(),
                workspace: "w".into(),
                name: "claude".into(),
                status: AgentStatus::Working,
            })
            .collect(),
        ..State::default()
    })
}

#[test]
fn a_worker_draws_reacts_to_state_and_messages() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_mod(root.path(), "counter", &counter("n="));
    let worker = Handle::spawn(dir).unwrap();
    let mut seen = Vec::new();
    assert_eq!(text(&next_tree(&worker, &mut seen)), "n=0/0");
    assert!(seen.iter().any(|report| report.manifest.is_some()));

    worker.offer(agents(2));
    assert_eq!(text(&next_tree(&worker, &mut seen)), "n=0/2");
    worker.send(Data::from("add")).unwrap();
    assert_eq!(text(&next_tree(&worker, &mut seen)), "n=1/2");
}

#[test]
fn a_click_may_run_a_command_but_a_timer_may_not() {
    let root = tempfile::tempdir().unwrap();
    let worker = Handle::spawn(write_mod(root.path(), "counter", &counter(""))).unwrap();
    let mut seen = Vec::new();
    next_tree(&worker, &mut seen);
    worker.send(Data::from("run")).unwrap();
    let commands = eventually(|| Some(worker.take().commands).filter(|c| !c.is_empty()));
    assert_eq!(commands, [herdr_commands::Command::NewWindow]);

    worker.send(Data::from("later")).unwrap();
    let refused = eventually(|| {
        let report = worker.take();
        assert!(report.commands.is_empty(), "a timer ran a command");
        report
            .log
            .into_iter()
            .find(|line| line.text.starts_with("herdr.run ignored"))
    });
    assert!(refused.text.contains("click"));
}

#[test]
fn saving_reloads_and_a_broken_save_keeps_the_last_tree() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_mod(root.path(), "live", &counter("one "));
    let worker = Handle::spawn(dir.clone()).unwrap();
    let mut seen = Vec::new();
    assert_eq!(text(&next_tree(&worker, &mut seen)), "one 0/0");

    fs::write(dir.join("main.luau"), counter("two  ")).unwrap();
    assert_eq!(text(&next_tree(&worker, &mut seen)), "two  0/0");

    fs::write(dir.join("main.luau"), "return herdr.mod {").unwrap();
    let error = eventually(|| worker.take().error.flatten());
    assert!(matches!(error, herdr_mods::Error::Lua(_)), "{error:?}");
    // The old mod still answers.
    worker.send(Data::from("add")).unwrap();
    assert_eq!(text(&next_tree(&worker, &mut seen)), "two  1/0");
}

#[test]
fn reload_reads_the_files_again_at_once() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_mod(root.path(), "live", &counter("a"));
    let worker = Handle::spawn(dir).unwrap();
    let mut seen = Vec::new();
    next_tree(&worker, &mut seen);
    worker.send(Data::from("add")).unwrap();
    assert_eq!(text(&next_tree(&worker, &mut seen)), "a1/0");
    worker.reload();
    // A reload starts the mod over.
    assert_eq!(text(&next_tree(&worker, &mut seen)), "a0/0");
}

#[test]
fn a_stuck_mod_never_blocks_the_window() {
    let root = tempfile::tempdir().unwrap();
    let main = "local ui = require(\"@herdr/ui\")\n\
        return herdr.mod { update = function() while true do end end,\n\
          view = function() return ui.spacer() end }";
    let worker = Handle::spawn(write_mod(root.path(), "stuck", main)).unwrap();
    let mut seen = Vec::new();
    next_tree(&worker, &mut seen);
    let start = Instant::now();
    worker.send(Data::from("x")).unwrap();
    worker.offer(agents(1));
    let _ = worker.take();
    drop(worker);
    assert!(start.elapsed() < Duration::from_millis(500));
}

#[test]
fn messages_are_bounded() {
    let root = tempfile::tempdir().unwrap();
    let main = "local ui = require(\"@herdr/ui\")\n\
        while true do end";
    let worker = Handle::spawn(write_mod(root.path(), "slow", main)).unwrap();
    let results = (0..100).map(|_| worker.send(Data::Nil)).collect::<Vec<_>>();
    assert!(
        results
            .iter()
            .any(|result| matches!(result, Err(Error::Busy(name)) if name == "slow"))
    );
}
