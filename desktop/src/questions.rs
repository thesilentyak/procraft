//! Questions the app asks in its own alert (see [`crate::alert`]), as the tablet asks them, rather
//! than in a system message box: a box's own buttons can't be named everywhere (Windows without its
//! newer dialogs shows OK and Cancel, so a choice can't be told apart, let alone made).
//! - Leaving an artwork that couldn't be saved (the storage full, say): Stay, in Cancel's place
//!   (as a tap beside the panel or Escape), or Leave Without Saving, red.
//! - Opening an artwork whose opening ended the app last time: Open Anyway, or Share Log, which
//!   shows the log to send on and asks again, or Cancel.

use crate::alert::{Alert, Answer, Asking};
use crate::look::Look;

/// What leaving the artwork open was for, done once the person chooses to leave it unsaved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    /// The window closing.
    Close,
    /// Back to the gallery.
    Gallery,
    /// A new artwork beside it.
    New,
    /// An artwork brought in, read meanwhile, opening in its place.
    Import,
}

/// Whether to leave an artwork that couldn't be saved without saving it.
#[derive(Default)]
pub struct LeaveQuestion {
    alert: Alert,
    then: Option<Then>,
}

impl LeaveQuestion {
    /// Asks, with `title` and what it says (`said`), whether to leave the artwork without saving it
    /// to do `then`.
    pub fn ask(&mut self, title: &str, said: &str, then: Then) {
        log::warn!("{title}: {said}");
        self.alert.ask(leave_asking(title, said));
        self.then = Some(then);
    }

    /// Whether it is being asked (nothing is left meanwhile).
    pub fn is_open(&self) -> bool {
        self.alert.is_open()
    }

    /// Draws it while it shows; once answered, what leaving was for, and whether to leave without
    /// saving (true) or stay (false: Stay, a tap beside it or Escape).
    pub fn show(&mut self, ctx: &egui::Context, look: &mut Look) -> Option<(Then, bool)> {
        let answer = self.alert.show(ctx, look)?;
        let then = self.then.take()?;
        Some((then, answer == Answer::Action))
    }
}

/// The question [`LeaveQuestion`] asks: Stay first (the default), Leave Without Saving red.
fn leave_asking(title: &str, said: &str) -> Asking {
    Asking {
        title: title.into(),
        message: said.into(),
        action: "Leave Without Saving".into(),
        icon: None,
        destructive: true,
        extra: None,
        first: Some("Stay".into()),
        alone: false,
    }
}

/// Whether to open an artwork whose opening ended the app last time (stopped for memory, or a
/// crash), so that it can't take the app down each time it is picked: `T` is the artwork asked
/// about.
pub struct OpenAnyway<T> {
    alert: Alert,
    artwork: Option<T>,
    /// The log of the run that ended while opening it, for Share Log.
    log: Option<std::path::PathBuf>,
}

impl<T> Default for OpenAnyway<T> {
    fn default() -> Self {
        OpenAnyway { alert: Alert::default(), artwork: None, log: None }
    }
}

impl<T> OpenAnyway<T> {
    /// Asks whether to open `artwork`, saying `said` (how that run ended, as its log tells); Share
    /// Log shows `log`, that run's log.
    pub fn ask(&mut self, artwork: T, said: String, log: std::path::PathBuf) {
        self.alert.ask(open_anyway_asking(said));
        self.artwork = Some(artwork);
        self.log = Some(log);
    }

    /// The log Share Log shows: that of the run that ended while opening the artwork asked about.
    pub fn log(&self) -> Option<&std::path::Path> {
        self.log.as_deref()
    }

    /// Draws it while it shows; once answered, the artwork to open, or none (Cancel, a tap beside
    /// it or Escape). Share Log shows the log to send on (`share_log`) and it asks again.
    pub fn show(&mut self, ctx: &egui::Context, look: &mut Look, share_log: impl FnOnce()) -> Option<Option<T>> {
        match self.alert.show(ctx, look)? {
            Answer::Action => Some(self.artwork.take()),
            Answer::Extra => {
                share_log();
                self.alert.reopen();
                None
            }
            Answer::Cancel | Answer::First => {
                self.artwork = None;
                Some(None)
            }
        }
    }
}

/// The question [`OpenAnyway`] asks, as the tablet's gallery asks it, saying `said` (how that run
/// ended, as its log tells: a crash, or not known); Share Log beside Open Anyway.
fn open_anyway_asking(said: String) -> Asking {
    Asking {
        title: procraft_core::opening_ended_app_title(),
        message: said,
        action: "Open Anyway".into(),
        icon: None,
        destructive: false,
        extra: Some("Share Log".into()),
        first: None,
        alone: false,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

    use super::*;

    /// A window running an alert's frames, with no window.
    pub(crate) struct Window {
        pub ctx: egui::Context,
        pub look: Look,
        time: f64,
    }

    impl Window {
        pub fn new() -> Window {
            let ctx = egui::Context::default();
            crate::look::install_fonts(&ctx);
            Window { ctx, look: Look::default(), time: 0.0 }
        }

        /// A frame of `show` with `events`: what it gave, and where each of `words` was drawn.
        pub fn frame<T>(
            &mut self,
            events: Vec<Event>,
            words: &[&str],
            mut show: impl FnMut(&egui::Context, &mut Look) -> Option<T>,
        ) -> (Option<T>, Vec<Option<Pos2>>) {
            self.time += 1.0 / 60.0;
            let raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1200.0, 800.0))),
                time: Some(self.time),
                events,
                ..RawInput::default()
            };
            let mut given = None;
            let look = &mut self.look;
            let mut output = self.ctx.clone().run_ui(raw, |ui| given = show(ui.ctx(), look));
            output.textures_delta.clear();
            let drawn = |w: &str| {
                output.shapes.iter().find_map(|c| match &c.shape {
                    egui::Shape::Text(t) if t.galley.text() == w => Some(t.pos + t.galley.size() / 2.0),
                    _ => None,
                })
            };
            (given, words.iter().map(|w| drawn(w)).collect())
        }
    }

    pub(crate) fn press(at: Pos2, pressed: bool) -> Event {
        Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
    }

    /// The leave question asked, come in whole, then `answer` given it (tapping where the words it
    /// names are drawn, or else a press of Escape or a tap beside the panel): what it answered, and
    /// whether its two buttons said Stay and Leave Without Saving.
    fn leave_answered(answer: &str) -> (Option<(Then, bool)>, bool) {
        let mut window = Window::new();
        let mut question = LeaveQuestion::default();
        question.ask("This artwork couldn't be saved", "Leaving without saving loses the changes.", Then::Gallery);
        assert!(question.is_open());
        let buttons = ["Stay", "Leave Without Saving", "OK", "Cancel"];
        window.frame(Vec::new(), &buttons, |ctx, look| question.show(ctx, look));
        std::thread::sleep(std::time::Duration::from_millis(350));
        let (_, drawn) = window.frame(Vec::new(), &buttons, |ctx, look| question.show(ctx, look));
        let named = drawn[0].is_some() && drawn[1].is_some() && drawn[2].is_none() && drawn[3].is_none();
        let events: Vec<Vec<Event>> = match answer {
            "Escape" => vec![vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }]],
            "beside" => {
                let at = pos2(20.0, 20.0);
                vec![vec![Event::PointerMoved(at)], vec![press(at, true)], vec![press(at, false)]]
            }
            words => {
                let at = drawn[buttons.iter().position(|b| *b == words).unwrap()].unwrap();
                vec![vec![Event::PointerMoved(at)], vec![press(at, true)], vec![press(at, false)]]
            }
        };
        let mut answered = None;
        for events in events.into_iter().chain([Vec::new(), Vec::new()]) {
            let (given, _) = window.frame(events, &[], |ctx, look| question.show(ctx, look));
            answered = answered.or(given);
        }
        assert!(!question.is_open(), "answered, it goes");
        (answered, named)
    }

    /// Leaving an artwork that couldn't be saved is asked in the app's own alert with its two real
    /// choices, so Leave Without Saving can be chosen on every system (a Windows message box
    /// showed OK and Cancel, both of which stayed); Stay, a tap beside it and Escape stay.
    #[test]
    fn leaving_unsaved_asks_stay_or_leave_without_saving_and_either_can_be_chosen() {
        let (left, named) = leave_answered("Leave Without Saving");
        assert!(named, "the buttons say Stay and Leave Without Saving, not OK and Cancel");
        assert_eq!(left, Some((Then::Gallery, true)));
        assert_eq!(leave_answered("Stay").0, Some((Then::Gallery, false)));
        assert_eq!(leave_answered("beside").0, Some((Then::Gallery, false)));
        assert_eq!(leave_answered("Escape").0, Some((Then::Gallery, false)));
    }

    /// Opening an artwork whose opening ended the app last time is asked in the app's own alert:
    /// Open Anyway, Share Log and Cancel, never Yes, No and Cancel (a Windows message box showed
    /// those, saying nothing of which was which). Share Log shows the log and asks again.
    #[test]
    fn opening_anyway_is_asked_with_its_own_buttons_and_share_log_asks_again() {
        let mut window = Window::new();
        let mut question = OpenAnyway::default();
        question.ask("artwork-1".to_owned(), procraft_core::opening_ended_app_said(false), std::path::PathBuf::new());
        let buttons = ["Open Anyway", "Share Log", "Cancel", "Yes", "No"];
        let mut shared = 0;
        let shown = |window: &mut Window, events, question: &mut OpenAnyway<String>, shared: &mut i32| {
            window.frame(events, &buttons, |ctx, look| question.show(ctx, look, || *shared += 1))
        };
        shown(&mut window, Vec::new(), &mut question, &mut shared);
        std::thread::sleep(std::time::Duration::from_millis(350));
        let (_, drawn) = shown(&mut window, Vec::new(), &mut question, &mut shared);
        assert!(drawn[..3].iter().all(Option::is_some), "{drawn:?}");
        assert!(drawn[3..].iter().all(Option::is_none), "{drawn:?}");
        let mut tap = |at: Pos2, question: &mut OpenAnyway<String>, shared: &mut i32| {
            let mut answered = None;
            for events in [vec![Event::PointerMoved(at)], vec![press(at, true)], vec![press(at, false)], vec![]] {
                answered = answered.or(shown(&mut window, events, question, shared).0);
            }
            answered
        };
        // Share Log: the log shown, still asking
        assert_eq!(tap(drawn[1].unwrap(), &mut question, &mut shared), None);
        assert_eq!(shared, 1);
        // Open Anyway: that artwork opens
        assert_eq!(tap(drawn[0].unwrap(), &mut question, &mut shared), Some(Some("artwork-1".to_owned())));
        // asked again and cancelled: none
        question.ask("artwork-2".to_owned(), procraft_core::opening_ended_app_said(false), std::path::PathBuf::new());
        std::thread::sleep(std::time::Duration::from_millis(350));
        assert_eq!(tap(drawn[2].unwrap(), &mut question, &mut shared), Some(None));
        assert_eq!(shared, 1);
    }
}
