use super::*;

#[test]
fn claude_runs_with_no_tools_mcp_servers_project_settings_or_session() {
    let claude = claude_args("Summarize");
    assert!(
        claude
            .windows(2)
            .any(|pair| pair[0] == "--tools" && pair[1].is_empty())
    );
    assert!(claude.contains(&"--no-session-persistence".into()));
    assert!(claude.contains(&"--strict-mcp-config".into()));
    assert!(
        claude
            .windows(2)
            .any(|pair| pair[0] == "--setting-sources" && pair[1] == "user")
    );
}

#[cfg(unix)]
fn sh(script: &str) -> (OsString, Vec<OsString>) {
    ("sh".into(), vec!["-c".into(), script.into()])
}

#[cfg(unix)]
#[test]
fn what_the_agent_leaves_running_does_not_hold_the_run() {
    let started = Instant::now();
    let text = run_first(
        [sh("cat > /dev/null; sleep 30 & echo Done.")],
        "x",
        WAIT,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(text, "Done.");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn the_answer_is_one_bounded_line() {
    let text = run_first(
        [sh("cat > /dev/null; printf '  Fixed the\\n  login.  '")],
        "Person: hi",
        WAIT,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(text, "Fixed the login.");
    let long = run_first(
        [sh("cat > /dev/null; yes word | head -c 20000")],
        "x",
        WAIT,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(long.chars().count(), MAX_ANSWER_CHARS);
}

#[cfg(unix)]
#[test]
fn a_missing_agent_falls_to_the_next() {
    let missing = ("sheprd-no-such-agent".into(), Vec::new());
    assert_eq!(
        run_first(
            [missing, sh("echo Done.")],
            "x",
            WAIT,
            &AtomicBool::new(false)
        )
        .unwrap(),
        "Done."
    );
    let only_missing = ("sheprd-no-such-agent".into(), Vec::new());
    assert!(matches!(
        run_first([only_missing], "x", WAIT, &AtomicBool::new(false)),
        Err(Error::NoAgent)
    ));
}

#[cfg(unix)]
#[test]
fn a_failing_agent_falls_to_the_next_but_a_slow_one_does_not() {
    assert_eq!(
        run_first(
            [sh("exit 55"), sh("echo Done.")],
            "x",
            WAIT,
            &AtomicBool::new(false)
        )
        .unwrap(),
        "Done."
    );
    assert_eq!(
        run_first(
            [sh("true"), sh("echo Done.")],
            "x",
            WAIT,
            &AtomicBool::new(false)
        )
        .unwrap(),
        "Done."
    );
    let slow = run_first(
        [sh("sleep 5"), sh("echo Done.")],
        "x",
        Duration::from_millis(200),
        &AtomicBool::new(false),
    );
    assert!(matches!(slow, Err(Error::TimedOut)));
    // The last failure is the one told.
    let missing = ("sheprd-no-such-agent".into(), Vec::new());
    assert!(matches!(
        run_first([missing, sh("exit 1")], "x", WAIT, &AtomicBool::new(false)),
        Err(Error::Failed)
    ));
}

#[cfg(unix)]
#[test]
fn failures_are_told_apart() {
    assert!(matches!(
        run_first([sh("exit 3")], "x", WAIT, &AtomicBool::new(false)),
        Err(Error::Failed)
    ));
    assert!(matches!(
        run_first([sh("printf '  '")], "x", WAIT, &AtomicBool::new(false)),
        Err(Error::Empty)
    ));
    let started = Instant::now();
    let slow = run_first(
        [sh("sleep 5")],
        "x",
        Duration::from_millis(200),
        &AtomicBool::new(false),
    );
    assert!(matches!(slow, Err(Error::TimedOut)));
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[cfg(unix)]
#[test]
fn a_cancelled_run_stops_at_once_with_what_it_started() {
    let cancelled = std::sync::Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let stopper = thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::Release);
    });
    let started = Instant::now();
    let stopped = run_first([sh("sleep 30 & sleep 30")], "x", WAIT, &cancelled);
    stopper.join().unwrap();
    assert!(matches!(stopped, Err(Error::Cancelled)));
    assert!(started.elapsed() < Duration::from_secs(10));
    // Already cancelled: nothing starts.
    assert!(matches!(
        run_first([sh("echo Done.")], "x", WAIT, &cancelled),
        Err(Error::Cancelled)
    ));
}
