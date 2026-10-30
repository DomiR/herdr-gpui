use crate::{Attachment, Core, End, Error, Result, wire::Attach};
use herdr_backend::{
    ClientMouseKind, ClientPaneInputEvent, Exit, Role, Size, SurfaceUpdate, TerminalId,
};
use herdr_terminal::{Damage, Emulator, Event, MouseReporting, Scroll};
use std::time::{Duration, Instant};

/// How long a synchronized update (DEC 2026) may hold the screen before it
/// is shown anyway, as terminals bound it.
const SYNC_TIMEOUT: Duration = Duration::from_millis(150);

/// An attached herdr-core terminal, emulated here: its raw output becomes
/// cells, and semantic input becomes bytes for its program's modes.
pub struct CoreTerminal {
    attachment: Attachment,
    emulator: Emulator,
    end: Option<End>,
    /// Whether the program read the mouse when the view was last told.
    reporting: Option<bool>,
    role: Role,
    /// When the synchronized update the emulator holds began.
    sync_since: Option<Instant>,
}

impl CoreTerminal {
    pub(super) fn attach(
        core: &Core,
        id: &TerminalId,
        size: Size,
        role: Role,
        scrollback: usize,
    ) -> Result<Self> {
        let attachment = core.attach(&Attach {
            cols: size.cols(),
            observe: role == Role::Observer,
            rows: size.rows(),
            terminal: id.as_str(),
            ..Attach::default()
        })?;
        // An observer sees the screen at the size its active viewer set.
        let emulated = match role {
            Role::Active => size,
            Role::Observer => {
                Size::new(attachment.terminal.cols, attachment.terminal.rows).unwrap_or(size)
            }
        };
        Ok(Self {
            attachment,
            emulator: Emulator::new(emulated, scrollback),
            end: None,
            reporting: None,
            role,
            sync_since: None,
        })
    }

    fn drives(&self) -> bool {
        self.role == Role::Active
    }

    fn release_stale_sync(&mut self) {
        if !self.emulator.sync_pending() {
            self.sync_since = None;
            return;
        }
        let since = *self.sync_since.get_or_insert_with(Instant::now);
        if since.elapsed() >= SYNC_TIMEOUT {
            self.emulator.end_sync();
            self.sync_since = None;
        }
    }
}

impl herdr_backend::Terminal for CoreTerminal {
    type Error = Error;

    /// A wheel turn the program does not read scrolls this view's history,
    /// an observer's too. Typing returns the view to the live screen.
    fn input(&mut self, input: &ClientPaneInputEvent) -> Result<()> {
        let bytes = self.emulator.encode(input);
        if bytes.is_empty() {
            if let ClientPaneInputEvent::Mouse { kind, lines, .. } = input {
                let lines = i32::from((*lines).max(1));
                match kind {
                    ClientMouseKind::ScrollUp => self.emulator.scroll(Scroll::Lines(lines)),
                    ClientMouseKind::ScrollDown => self.emulator.scroll(Scroll::Lines(-lines)),
                    _ => {}
                }
            }
            return Ok(());
        }
        if !self.drives() {
            return Ok(());
        }
        if !matches!(input, ClientPaneInputEvent::Mouse { .. })
            && self.emulator.scroll_metrics().offset > 0
        {
            self.emulator.scroll(Scroll::Bottom);
        }
        self.attachment.write(&bytes)
    }

    /// An active view sizes the program's terminal; an observer only
    /// follows the size the terminal already has, as its `size` events say.
    fn resize(&mut self, size: Size) -> Result<()> {
        self.emulator.resize(size);
        if !self.drives() {
            return Ok(());
        }
        self.attachment.resize(size.cols(), size.rows())
    }

    /// The core reports focus to programs that asked for it.
    fn focus(&mut self, focused: bool) -> Result<()> {
        if !self.drives() {
            return Ok(());
        }
        self.attachment.focus(focused)
    }

    fn next(&mut self, timeout: Duration) -> Result<Vec<SurfaceUpdate>> {
        match &self.end {
            Some(End::Lost) => return Err(Error::Lost),
            Some(End::Closed(_)) => return Ok(Vec::new()),
            None => {}
        }
        // A held update is released on time even when no bytes follow it.
        let timeout = match self.sync_since {
            Some(since) => {
                timeout.min((since + SYNC_TIMEOUT).saturating_duration_since(Instant::now()))
            }
            None => timeout,
        };
        let chunk = self.attachment.read(timeout)?;
        self.emulator.feed(&chunk.bytes);
        self.release_stale_sync();
        let mut updates: Vec<SurfaceUpdate> = self
            .emulator
            .take_events()
            .into_iter()
            .map(|event| match event {
                Event::Bell { .. } => SurfaceUpdate::Bell,
                Event::Title(title) => SurfaceUpdate::Title(title.unwrap_or_default()),
            })
            .collect();
        let reporting = self.emulator.modes().mouse != MouseReporting::Off;
        if self.reporting != Some(reporting) {
            self.reporting = Some(reporting);
            updates.push(SurfaceUpdate::MouseReporting(reporting));
        }
        match self.emulator.take_damage() {
            Damage::Full(frame) => updates.push(SurfaceUpdate::Frame(frame)),
            damage if damage.is_empty() => {}
            Damage::Rows { rows, cursor } => updates.push(SurfaceUpdate::Rows { cursor, rows }),
        }
        match chunk.end {
            Some(End::Closed(closed)) => {
                updates.push(SurfaceUpdate::Ended(Exit {
                    code: closed.exit_code,
                }));
                self.end = Some(End::Closed(closed));
            }
            // What arrived before the loss is still shown; the next call fails.
            Some(End::Lost) if updates.is_empty() => return Err(Error::Lost),
            Some(End::Lost) => self.end = Some(End::Lost),
            None => {}
        }
        Ok(updates)
    }
}
