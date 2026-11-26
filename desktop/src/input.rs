//! The window's pointers handed to the session's touches (crates/core's `touch`, shared with the
//! Android app), which paint, move the canvas and say what the app should do. The mouse adds what
//! it can't do by touch: the wheel zooms, and the middle button (or space) moves the canvas; on a
//! model it turns it, as a finger does, and with Shift moves it. Dragged with Space and Ctrl (⌘)
//! held it zooms the canvas, with R held it turns it (with Shift in 15° steps), and R tapped turns
//! it a step (the core's key drags, keys.rs). A pen with those keys held does as the mouse does;
//! otherwise a pen and fingers go on as they do.
//!
//! What paints (the mouse pointer, which a tablet's pen moves under X11 and on a Mac, and a pen's
//! touches) reports hundreds of moves a second, each its own window event. Their samples wait,
//! each read as it came, and go to the canvas together once a frame (see [`Input::flush`]), as a
//! tablet's batched samples do: a stroke's step, and the tip it shows, then cost once a frame
//! rather than once a move.

use std::time::{Duration, Instant};

use procraft_core::{CanvasSession, KeyDrag, PenSample, PointerKind, TouchCommand};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

/// The eyedropper's window, points.
pub const PICK_WINDOW: f32 = 54.0;
/// How much a notch of the wheel zooms by.
const WHEEL_ZOOM: f32 = 1.15;
/// How much a pixel of scrolling zooms by, with Ctrl (or ⌘) held.
const SCROLL_ZOOM: f32 = 0.005;
/// How far a notch of the wheel turns the canvas with Shift held (radians, 5°).
const WHEEL_TURN: f32 = 5.0 * std::f32::consts::PI / 180.0;
/// How far a pixel of scrolling turns it with Shift held (radians, a quarter of a degree).
const SCROLL_TURN: f32 = 0.25 * std::f32::consts::PI / 180.0;

/// R let go within this, with no drag, is a tap: it turns the canvas a step (the touches' tap).
const R_TAP: Duration = Duration::from_millis(300);

/// The mouse's pointer id: touches have the platform's.
const MOUSE: u64 = u64::MAX;

/// A finger's touch heard from landing to lifting in less than this (milliseconds, not counting
/// the time the canvas spent on it) came whole: no finger lifts that fast, so it was held back and
/// handed on at once (see [`Input::hold_whole_touches`]).
const WHOLE_TOUCH_MS: f64 = 4.0;
/// A finger's touch that came whole lifts once no other has landed for this long (milliseconds):
/// the fingers of a tap a window manager hands on one after the other lift together.
const HELD_LIFT_MS: f64 = 120.0;

/// Whether the window's touches may come held back and whole (an X11 window), for each [`Input`]
/// made from now on.
static WHOLE_TOUCHES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The window is an X11 one (`on`), whose window manager may hold each touch back and hand it on
/// whole (see [`Input::hold_whole_touches`]), or isn't.
pub fn set_whole_touches(on: bool) {
    WHOLE_TOUCHES.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// A finger's touch down on the canvas: how long it has been heard from, without the canvas's own
/// time on it, and when that last ended (milliseconds).
#[derive(Clone, Copy, Debug)]
struct FingerHeard {
    id: u64,
    idle_ms: f64,
    done_ms: f64,
}
/// How many moves a stroke the mouse pointer began with no tablet pen read waits for the pen's
/// reading, to be the pen's: a reading late at all comes a frame or two after the press.
const UNREAD_MOVES: u32 = 4;

/// Which touch the interface follows as its pointer. egui follows the first touch down and no other
/// until it lifts, so a hand resting on the canvas would keep a finger off the panels and the side
/// bar. A touch landing on the interface while the pointer's touch rests on the canvas takes the
/// pointer over; the resting one is cancelled for the interface and unheard by it from then on
/// (the canvas goes on hearing it as before).
#[derive(Default)]
pub struct InterfaceTouches {
    /// The pointer's touch as it last came, and whether it landed on the canvas.
    pointer: Option<(Touch, bool)>,
    set_aside: Vec<u64>,
}

impl InterfaceTouches {
    /// What of `event` the interface hears, in order (`on_interface`: whether a point is on the
    /// interface rather than the canvas): nothing, the event, or the resting touch cancelled and then
    /// the event.
    pub fn route(&mut self, event: WindowEvent, on_interface: impl Fn((f32, f32)) -> bool) -> Vec<WindowEvent> {
        let WindowEvent::Touch(touch) = event else { return vec![event] };
        if self.set_aside.contains(&touch.id) {
            if matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                self.set_aside.retain(|id| *id != touch.id);
            }
            return Vec::new();
        }
        let following = self.pointer.is_some_and(|(p, _)| p.id == touch.id);
        let mut heard = Vec::new();
        match touch.phase {
            TouchPhase::Started => {
                let on_canvas = !on_interface((touch.location.x as f32, touch.location.y as f32));
                match self.pointer {
                    None => self.pointer = Some((touch, on_canvas)),
                    Some((resting, true)) if !on_canvas => {
                        heard.push(WindowEvent::Touch(Touch { phase: TouchPhase::Cancelled, ..resting }));
                        self.set_aside.push(resting.id);
                        self.pointer = Some((touch, false));
                    }
                    Some(_) => {}
                }
            }
            TouchPhase::Moved if following => self.pointer = self.pointer.map(|(_, on)| (touch, on)),
            TouchPhase::Ended | TouchPhase::Cancelled if following => self.pointer = None,
            _ => {}
        }
        heard.push(WindowEvent::Touch(touch));
        heard
    }
}

/// Whether the interface has been told it is on a touch screen, for a tablet's pen that moves the
/// mouse pointer. egui scrolls a list by dragging its contents only on a touch screen, which it
/// takes one to be once it has had a touch: the system's pen comes as touches, but a tablet's pen
/// that moves the mouse pointer (Windows Ink off, read through WinTab; X11; a Mac) makes none, so
/// the lists stayed still under it. The first time such a pen is seen the interface is told of a
/// touch of its own, cancelled as it comes, so it presses nothing: the lists then drag with that
/// pen as with the system's.
#[derive(Default)]
pub struct PenTouchScreen {
    told: bool,
}

impl PenTouchScreen {
    /// A touch no finger or pen makes: a device and an id of its own.
    const TOUCH: u64 = u64::MAX;

    /// The interface's input for a frame, `pen_moves_mouse` while a tablet's pen moves the mouse
    /// pointer: the first time one does, with the touch that tells it it is on a touch screen.
    pub fn tell(&mut self, raw: &mut egui::RawInput, pen_moves_mouse: bool) {
        if pen_moves_mouse && !std::mem::replace(&mut self.told, true) {
            raw.events.push(egui::Event::Touch {
                device_id: egui::TouchDeviceId(Self::TOUCH),
                id: egui::TouchId(Self::TOUCH),
                phase: egui::TouchPhase::Cancel,
                pos: egui::Pos2::ZERO,
                force: None,
            });
        }
    }
}

/// The interface's input, frame by frame, for a pen that comes to `from`, is pressed, dragged `by`
/// in ten steps and lifted: as the system's pen gives it (`touches`: its touches, which egui-winit
/// also makes its pointer's), or as a tablet's pen moving the mouse pointer does (the pointer
/// alone).
#[cfg(test)]
pub fn pen_drag(from: egui::Pos2, by: egui::Vec2, touches: bool) -> Vec<Vec<egui::Event>> {
    use egui::{Event, PointerButton, TouchPhase};
    let touch = |phase, pos| Event::Touch {
        device_id: egui::TouchDeviceId(1),
        id: egui::TouchId(1),
        phase,
        pos,
        force: Some(0.5),
    };
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut frames = Vec::new();
    if touches {
        frames.push(vec![touch(TouchPhase::Start, from), Event::PointerMoved(from), button(from, true)]);
    } else {
        frames.push(vec![Event::PointerMoved(from)]);
        frames.push(vec![button(from, true)]);
    }
    let mut at = from;
    for _ in 0..10 {
        at += by / 10.0;
        let moved = Event::PointerMoved(at);
        frames.push(if touches { vec![touch(TouchPhase::Move, at), moved] } else { vec![moved] });
    }
    frames.push(if touches {
        vec![touch(TouchPhase::End, at), button(at, false), Event::PointerGone]
    } else {
        vec![button(at, false)]
    });
    frames
}

pub struct Input {
    clock: Instant,
    /// What the touches asked, for the app to do at the next frame.
    pub commands: Vec<TouchCommand>,
    /// A press went to the canvas since the interface last looked.
    pub canvas_pressed: bool,
    /// The pointers down on the canvas.
    down: Vec<u64>,
    /// Pointers down that the canvas didn't take (pressed on the interface, or on the canvas
    /// while a popover was open, which that press only closes): while one is, no brush outline
    /// hovers (it would stay where the pen went down, as the pen moves on).
    ignored: Vec<u64>,
    /// The mouse: where it is, whether it is over the window, and whether it (or space) moves the
    /// view.
    cursor: Option<(f32, f32)>,
    inside: bool,
    panning: bool,
    /// The mouse dragged with a key held, zooming or turning the canvas (the core's key drag).
    key_drag: Option<KeyDrag>,
    /// The pen moving the view (pressed with space or R held), as the mouse does.
    pen_panning: Option<u64>,
    /// A tablet's pen pressed the mouse pointer (X11, a Mac): its pressure as last read, for the
    /// stroke's moves while it goes unread a moment (another pointer heard from in between).
    pen_pressure: Option<f32>,
    /// The mouse pointer pressed on the canvas with no tablet pen read there: where, and how many
    /// moves since, while it may still turn out to be the pen's (see [`Input::pen_came`]).
    unread_press: Option<((f32, f32), u32)>,
    /// Where the tablet pen moving the mouse pointer is read: the pen's own readings (a test's).
    read_mouse_pen: fn((f32, f32)) -> Option<crate::pen::MousePen>,
    /// The brush cursor follows the mouse hovering, so its moves draw a frame.
    pub hover_shown: bool,
    space: bool,
    /// Space has moved or zoomed the canvas since it went down; and it was tapped, doing neither
    /// (taken by the interface as the QuickMenu).
    space_used: bool,
    pub space_tapped: bool,
    /// R held for the canvas (not typed) since when, and whether the mouse went down since (then
    /// it turned the canvas by dragging, and letting go turns it no step).
    r_key: Option<Instant>,
    r_dragged: bool,
    command_key: bool,
    /// Ctrl and Alt held (on a Mac, Space with Ctrl zooms as with ⌘: the system may keep either).
    control: bool,
    alt: bool,
    shift: bool,
    /// The interface takes the keys (text being edited, the hex keypad): R is typed, not a turn.
    pub keys_taken: bool,
    /// A model is shown (not its 2D texture): dragging the view turns it, as one finger does (the
    /// touches').
    pub turns_model: bool,
    /// The moves of what paints heard since the canvas last took them: each pointer's samples in
    /// order, each read as its move came (where, its pressure and lean then, and when).
    waiting: Vec<(u64, Vec<PenSample>)>,
    /// How many times moves went to the canvas, and how many samples they took there.
    #[cfg_attr(not(test), allow(dead_code))]
    handed: Handed,
    /// Touches may come held back and whole. Under X11 a window manager can take every touch first
    /// (Cinnamon's does, for its own gestures: each until it has moved 30 px, lifted, or 150 ms
    /// have gone) and then hand each to the window by itself, all of it at once. Fingers tapped
    /// together then come one after the other, each landed and lifted, and make no tap together.
    /// So a finger that comes whole and alone has its lift held back until no other finger has
    /// landed for [`HELD_LIFT_MS`]: the fingers that came that way lift together, as they did.
    pub hold_whole_touches: bool,
    /// The fingers down on the canvas, as heard (see [`FingerHeard`]).
    fingers: Vec<FingerHeard>,
    /// Fingers that came whole and lifted, their lifts held back (where each lifted), and when
    /// those go on (milliseconds).
    held_lifts: Vec<(u64, (f32, f32))>,
    held_until: Option<f64>,
    /// The time samples are read at in place of the clock's, milliseconds: a test's own.
    #[cfg(test)]
    test_clock: Option<f64>,
}

/// Moves handed to the canvas: how many times, and how many samples in all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct Handed {
    pub batches: usize,
    pub samples: usize,
}

impl Default for Input {
    fn default() -> Input {
        Input {
            clock: Instant::now(),
            commands: Vec::new(),
            canvas_pressed: false,
            down: Vec::new(),
            ignored: Vec::new(),
            cursor: None,
            inside: false,
            panning: false,
            key_drag: None,
            pen_panning: None,
            pen_pressure: None,
            unread_press: None,
            read_mouse_pen: crate::pen::mouse_pen_at,
            hover_shown: false,
            space: false,
            space_used: false,
            space_tapped: false,
            r_key: None,
            r_dragged: false,
            command_key: false,
            control: false,
            alt: false,
            shift: false,
            keys_taken: false,
            turns_model: false,
            waiting: Vec::new(),
            handed: Handed::default(),
            hold_whole_touches: WHOLE_TOUCHES.load(std::sync::atomic::Ordering::Relaxed),
            fingers: Vec::new(),
            held_lifts: Vec::new(),
            held_until: None,
            #[cfg(test)]
            test_clock: None,
        }
    }
}

impl Input {
    /// Where the mouse hovers (window pixels): over the window, nothing pressed, the view not
    /// moving.
    /// Where the mouse (or a pen moving the pointer) last was over the window, window pixels.
    pub fn cursor(&self) -> Option<(f32, f32)> {
        self.cursor
    }

    /// Whether the mouse is over the window.
    pub fn inside(&self) -> bool {
        self.inside
    }

    pub fn hovering(&self) -> Option<(f32, f32)> {
        self.cursor.filter(|_| self.inside && self.down.is_empty() && self.ignored.is_empty() && !self.moving_view())
    }

    /// Nothing is pressed on the canvas and the view isn't moving.
    pub fn idle(&self) -> bool {
        self.down.is_empty() && !self.panning && self.key_drag.is_none()
    }

    /// Something is pressed on the canvas (a pen, the mouse, a finger), and not to move the view.
    pub fn pressing(&self) -> bool {
        !self.down.is_empty() && !self.moving_view()
    }

    /// What is painting isn't under the hand (a mouse, a tablet's pen without a screen, as far as
    /// the system says): only the screen can show where it is.
    pub fn pressing_from_afar(&self) -> bool {
        self.pressing() && self.down.iter().any(|&id| id == MOUSE || crate::pen::away_from_screen(id))
    }

    /// Alt held as the modify button is (Ctrl with it is AltGr, typing).
    pub fn modify_key(&self) -> bool {
        self.alt && !self.control
    }

    /// Space or R is held, or the mouse moves the view.
    pub fn moving_view(&self) -> bool {
        self.space || self.r_key.is_some() || self.panning || self.key_drag.is_some()
    }

    /// The mouse zooms the view, or would if pressed.
    pub fn zooming_view(&self) -> bool {
        self.key_drag.or_else(|| self.drag_held()) == Some(KeyDrag::Zoom)
    }

    /// The drag the keys held make of the mouse pressed on the canvas: R turns it, Space with
    /// Ctrl (⌘) zooms it (Space alone moves it, as the middle button does).
    fn drag_held(&self) -> Option<KeyDrag> {
        if self.r_key.is_some() {
            Some(KeyDrag::Turn)
        } else if self.space && (self.command_key || self.control) {
            Some(KeyDrag::Zoom)
        } else {
            None
        }
    }

    /// A press at `p` with keys held begins the drag they make of it, if they make one. True if
    /// they do.
    fn begin_key_drag(&mut self, session: &CanvasSession, p: (f32, f32)) -> bool {
        let Some(how) = self.drag_held() else { return false };
        session.key_drag_begin(how, p.0, p.1);
        self.key_drag = Some(how);
        self.space_used = true;
        self.r_dragged |= how == KeyDrag::Turn;
        true
    }

    fn end_key_drag(&mut self, session: &CanvasSession) {
        if self.key_drag.take().is_some() {
            session.key_drag_end();
        }
    }

    fn now_ms(&self) -> f64 {
        #[cfg(test)]
        if let Some(ms) = self.test_clock {
            return ms;
        }
        self.clock.elapsed().as_secs_f64() * 1000.0
    }

    /// When the touches next need [`Input::tick`], if ever (a held lift too).
    pub fn next_deadline(&self, session: &CanvasSession) -> Option<Instant> {
        let due = match (session.touch_deadline(), self.held_until) {
            (Some(a), Some(b)) => a.min(b),
            (a, b) => a.or(b)?,
        };
        Some(self.clock + Duration::from_secs_f64((due / 1000.0).max(0.0)))
    }

    /// Runs the touches' waits that have run out. True if anything happened. The moves waiting go
    /// first when one has, as they came before now: a pen still moving holds no QuickShape, and an
    /// airbrush held still lays its latest again, not one from before them.
    pub fn tick(&mut self, session: &CanvasSession) -> bool {
        let now = self.now_ms();
        let lifted = self.held_until.is_some_and(|until| until <= now) && self.lift_held(session);
        let due = !self.waiting.is_empty() && session.touch_deadline().is_some_and(|due| due <= now);
        let flushed = due && self.flush(session);
        let out = session.touch_tick(now);
        self.take(out) || flushed || lifted
    }

    /// The lifts held back go on, in the order they came. True if there were any.
    fn lift_held(&mut self, session: &CanvasSession) -> bool {
        self.held_until = None;
        let held = std::mem::take(&mut self.held_lifts);
        for &(id, at) in &held {
            self.lifted(session, id, at, 1.0, false);
        }
        !held.is_empty()
    }

    /// Finger `id`'s touch is heard from at `now` (milliseconds): the time since it was last
    /// counts as heard, and its time on the canvas starts.
    fn finger_heard(&mut self, id: u64, now: f64) -> Option<FingerHeard> {
        let f = self.fingers.iter_mut().find(|f| f.id == id)?;
        f.idle_ms += now - f.done_ms;
        f.done_ms = now;
        Some(*f)
    }

    /// The canvas is done with finger `id`'s touch for now: its time from here counts as heard.
    fn finger_done(&mut self, id: u64) {
        let now = self.now_ms();
        if let Some(f) = self.fingers.iter_mut().find(|f| f.id == id) {
            f.done_ms = now;
        }
    }

    /// The moves waiting go to the canvas, each pointer's samples together, as a tablet's batched
    /// samples do: once a frame, before the interface looks at what the touches asked, and before
    /// anything else the input does, so all of it comes in the order it was made. True if there
    /// were any.
    pub fn flush(&mut self, session: &CanvasSession) -> bool {
        if self.waiting.is_empty() {
            return false;
        }
        for (id, samples) in std::mem::take(&mut self.waiting) {
            self.handed.batches += 1;
            self.handed.samples += samples.len();
            let out = session.touch_move(id, samples);
            self.take(out);
        }
        true
    }

    /// The moves handed to the canvas so far.
    #[cfg(test)]
    pub fn handed(&self) -> Handed {
        self.handed
    }

    /// Whether `event` leaves the moves waiting as they are: a move of what paints (which waits
    /// too), or one nothing here heeds (X11 gives each axis of a pen's move as an event of its own).
    fn waits(&self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::CursorMoved { .. } => self.down.contains(&MOUSE) && self.key_drag.is_none() && !self.panning,
            WindowEvent::Touch(t) => {
                t.phase == TouchPhase::Moved
                    && self.down.contains(&t.id)
                    && self.pen_panning != Some(t.id)
                    && pen_pressure(t).is_some()
            }
            WindowEvent::AxisMotion { .. } => true,
            _ => false,
        }
    }

    fn take(&mut self, out: Vec<TouchCommand>) -> bool {
        let any = !out.is_empty();
        self.commands.extend(out);
        any
    }

    /// What the mouse pointer is now, at `at`: a tablet's pen moving it (X11 gives a pen as the
    /// mouse), as a pen or its eraser end with its pressure there; else the mouse.
    fn mouse_kind(&self, at: (f32, f32)) -> (PointerKind, f32) {
        match (self.read_mouse_pen)(at) {
            Some(pen) => (if pen.eraser { PointerKind::StylusEraser } else { PointerKind::Stylus }, pen.pressure),
            None => (PointerKind::Mouse, 1.0),
        }
    }

    /// The pressure of the mouse pointer's sample at `at`: a stroke a pen began keeps the pen's
    /// last pressure through a moment it goes unread, rather than jumping to full as the mouse's.
    fn mouse_pressure(&mut self, at: (f32, f32)) -> f32 {
        let read = (self.read_mouse_pen)(at).map(|pen| pen.pressure);
        if self.pen_pressure.is_some() && read.is_some() {
            self.pen_pressure = read;
        }
        read.or(self.pen_pressure).unwrap_or(1.0)
    }

    /// The mouse pointer pressed on the canvas, now at `at`: if it went down with no tablet pen
    /// read and the pen's reading has come since, within the stroke's first moves, the stroke is
    /// begun again as the pen's, where it went down. The pen's reading may come just after the
    /// press it made (the driver's messages crossing, or its packets only now coming to this
    /// window, as it comes to the front): taken for the mouse's, the stroke would have no pressure.
    fn pen_came(&mut self, session: &CanvasSession, at: (f32, f32)) {
        let Some((down, moves)) = self.unread_press.take() else { return };
        if (self.read_mouse_pen)(at).is_none() {
            self.unread_press = (moves < UNREAD_MOVES).then_some((down, moves + 1));
            return;
        }
        let (kind, pressure) = self.mouse_kind(at);
        // (the moves waiting were the stroke taken back)
        self.waiting.retain(|(id, _)| *id != MOUSE);
        let out = session.touch_cancel_one(MOUSE);
        self.take(out);
        self.pen_pressure = Some(pressure);
        self.pressed(session, MOUSE, kind, down, pressure);
    }

    /// A sample of pointer `id` at `at`: a pen's with its lean, if the system gave one, and a
    /// tablet's pen moving the mouse pointer where its tablet says it was, finer than the pointer's
    /// whole pixels (with no smoothing a slow line through them goes in steps).
    fn sample(&self, id: u64, at: (f32, f32), pressure: f32) -> PenSample {
        let (x, y) = if id == MOUSE { crate::pen::mouse_pen_fine_at(at).unwrap_or(at) } else { at };
        let (tilt, orientation) = crate::pen::lean(id).unwrap_or((0.0, 0.0));
        let roll = crate::pen::roll(id).unwrap_or(0.0);
        PenSample { x, y, pressure: pressure.clamp(0.0, 1.0), tilt, orientation, time_ms: self.now_ms(), roll }
    }

    fn pressed(&mut self, session: &CanvasSession, id: u64, kind: PointerKind, at: (f32, f32), pressure: f32) {
        self.down.retain(|d| *d != id);
        self.down.push(id);
        self.canvas_pressed = true;
        // a pen's stroke goes by its lean only if its driver says how it leans
        if matches!(kind, PointerKind::Stylus | PointerKind::StylusEraser) {
            session.set_pen_tilt_known(crate::pen::lean(id).is_some());
        }
        let out = session.touch_down(id, kind, vec![self.sample(id, at, pressure)]);
        self.take(out);
    }

    /// What paints, pointer `id`, moved to `at`: its sample, read now, waits for the frame (see
    /// [`Input::flush`]).
    fn moved(&mut self, id: u64, at: (f32, f32), pressure: f32) {
        let sample = self.sample(id, at, pressure);
        match self.waiting.iter_mut().find(|(waiting, _)| *waiting == id) {
            Some((_, samples)) => samples.push(sample),
            None => self.waiting.push((id, vec![sample])),
        }
    }

    fn lifted(&mut self, session: &CanvasSession, id: u64, at: (f32, f32), pressure: f32, cancelled: bool) {
        self.down.retain(|d| *d != id);
        let out = if cancelled {
            session.touch_cancel_one(id)
        } else {
            session.touch_up(id, vec![self.sample(id, at, pressure)])
        };
        self.take(out);
    }

    /// Acts on `event` (`consumed` when the interface took it; `over` tells whether a point is on
    /// the interface rather than the canvas). True if anything changed. Presses go by `over` alone:
    /// `consumed` is worked out from the last frame, so a press that comes straight after the
    /// pointer (or a finger) arrives from the interface would count as the interface's.
    pub fn on_event(
        &mut self,
        session: &CanvasSession,
        event: &WindowEvent,
        consumed: bool,
        over: impl Fn((f32, f32)) -> bool,
    ) -> bool {
        // what paints moving waits for the frame; anything else comes after the moves before it
        if !self.waits(event) {
            self.flush(session);
        }
        match event {
            WindowEvent::ModifiersChanged(m) => {
                let state = m.state();
                self.command_key = if cfg!(target_os = "macos") { state.super_key() } else { state.control_key() };
                self.control = state.control_key();
                self.alt = state.alt_key();
                let shift = std::mem::replace(&mut self.shift, state.shift_key());
                // a turn goes onto its steps as Shift goes down, and off them as it comes up
                if let (Some(KeyDrag::Turn), Some(p), true) = (self.key_drag, self.cursor, shift != self.shift) {
                    session.key_drag_move(p.0, p.1, self.shift);
                    return true;
                }
                false
            }
            WindowEvent::KeyboardInput { event, .. } if event.logical_key == Key::Named(NamedKey::Space) => {
                let held = event.state == ElementState::Pressed && !consumed;
                if held && !self.space {
                    self.space_used = self.command_key || self.control;
                }
                self.space_tapped |= self.space && !held && !self.space_used;
                self.space = held;
                if !self.space && self.panning && !self.down.contains(&MOUSE) {
                    self.panning = false;
                    self.pen_panning = None;
                }
                // a zoom ends with it too
                if !self.space && self.key_drag == Some(KeyDrag::Zoom) {
                    self.end_key_drag(session);
                }
                true
            }
            WindowEvent::KeyboardInput { event, is_synthetic, .. } if is_r(event) => {
                match event.state {
                    // (a press as the window comes to the front is a key held from before: no tap)
                    ElementState::Pressed if !event.repeat && !*is_synthetic => {
                        let typed = consumed || self.keys_taken || self.command_key || self.control || self.alt;
                        self.r_key = (!typed).then(Instant::now);
                        self.r_dragged = false;
                    }
                    ElementState::Pressed => {}
                    ElementState::Released => {
                        // tapped, it turns the canvas a step (with Shift back); a drag turned it
                        let tapped = self.r_key.take().is_some_and(|since| since.elapsed() < R_TAP);
                        if tapped && !self.r_dragged && !consumed {
                            session.key_turn(self.shift);
                        }
                        if self.key_drag == Some(KeyDrag::Turn) {
                            self.end_key_drag(session);
                        }
                    }
                }
                true
            }
            WindowEvent::Focused(false) => {
                // keys let go elsewhere never come back up here (nor, perhaps, a press's lift)
                self.space = false;
                self.r_key = None;
                self.command_key = false;
                self.control = false;
                self.alt = false;
                self.shift = false;
                self.end_key_drag(session);
                // (the pen's lift, like a key's, may never come)
                if self.pen_panning.take().is_some() {
                    self.panning = false;
                }
                self.ignored.clear();
                false
            }
            WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } => {
                self.inside = matches!(event, WindowEvent::CursorEntered { .. });
                self.hover_shown
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = (position.x as f32, position.y as f32);
                self.inside = true;
                if self.key_drag.is_some() {
                    self.cursor = Some(p);
                    session.key_drag_move(p.0, p.1, self.shift);
                    return true;
                }
                if self.pan_to(session, p) {
                    return true;
                }
                if self.down.contains(&MOUSE) {
                    self.pen_came(session, p);
                    let pressure = self.mouse_pressure(p);
                    self.moved(MOUSE, p, pressure);
                    return true;
                }
                // a spray can's sound (a plugin's) rattles shaken above the canvas
                session.sound_hover(p.0, p.1);
                self.hover_shown
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(p) = self.cursor else { return false };
                match (button, state) {
                    // with R held it turns the canvas, with Space and Ctrl (⌘) zooms it: nothing
                    // is painted
                    (MouseButton::Left, ElementState::Pressed)
                        if self.down.is_empty() && !self.panning && !over(p) && self.drag_held().is_some() =>
                    {
                        self.begin_key_drag(session, p);
                        false
                    }
                    (MouseButton::Left, ElementState::Released) if self.key_drag.is_some() => {
                        self.end_key_drag(session);
                        false
                    }
                    (MouseButton::Left | MouseButton::Middle, ElementState::Pressed)
                        if self.down.is_empty() && !over(p) && (self.space || *button == MouseButton::Middle) =>
                    {
                        self.panning = true;
                        self.space_used = true;
                        false
                    }
                    (MouseButton::Left, ElementState::Pressed) if !self.panning && !over(p) => {
                        let (kind, pressure) = self.mouse_kind(p);
                        self.pen_pressure = (kind != PointerKind::Mouse).then_some(pressure);
                        self.unread_press = (kind == PointerKind::Mouse).then_some((p, 0));
                        self.pressed(session, MOUSE, kind, p, pressure);
                        true
                    }
                    (MouseButton::Left, ElementState::Released) if self.down.contains(&MOUSE) => {
                        self.pen_came(session, p);
                        self.unread_press = None;
                        let pressure = self.mouse_pressure(p);
                        self.pen_pressure = None;
                        self.lifted(session, MOUSE, p, pressure, false);
                        true
                    }
                    (MouseButton::Left | MouseButton::Middle, ElementState::Released) => {
                        self.panning = false;
                        false
                    }
                    _ => false,
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(p) = self.cursor.filter(|p| !consumed && !over(*p)) else { return false };
                // with Shift the wheel turns the canvas about the pointer, up clockwise (the
                // desktop's own, for a mouse or a pen without touch; touch and trackpads turn it
                // as on the tablet); some systems send Shift's wheel sideways
                if self.shift {
                    let turn = match delta {
                        MouseScrollDelta::LineDelta(x, y) => (if *y == 0.0 { *x } else { *y }) * WHEEL_TURN,
                        MouseScrollDelta::PixelDelta(d) => (if d.y == 0.0 { d.x } else { d.y }) as f32 * SCROLL_TURN,
                    };
                    session.view_gesture(p.0, p.1, 0.0, 0.0, 1.0, turn);
                    return true;
                }
                match delta {
                    MouseScrollDelta::LineDelta(_, y) => {
                        session.view_gesture(p.0, p.1, 0.0, 0.0, WHEEL_ZOOM.powf(*y), 0.0);
                    }
                    MouseScrollDelta::PixelDelta(d) if self.command_key => {
                        session.view_gesture(p.0, p.1, 0.0, 0.0, (d.y as f32 * SCROLL_ZOOM).exp(), 0.0);
                    }
                    MouseScrollDelta::PixelDelta(d) => {
                        session.view_gesture(p.0, p.1, d.x as f32, d.y as f32, 1.0, 0.0);
                    }
                }
                true
            }
            WindowEvent::PinchGesture { delta, phase, .. } => {
                let Some(p) = self.cursor.filter(|p| !over(*p)) else { return false };
                session.view_gesture(p.0, p.1, 0.0, 0.0, (1.0 + *delta as f32).max(0.01), 0.0);
                // let go near a right angle, the canvas turns onto it, as after fingers (unless a
                // plugin keeps gestures from turning it)
                if *phase == TouchPhase::Ended && !session.gesture_turn_locked() {
                    session.settle_turn();
                }
                true
            }
            WindowEvent::RotationGesture { delta, phase, .. } => {
                let Some(p) = self.cursor.filter(|p| !over(*p)) else { return false };
                // a plugin may keep gestures from turning the canvas
                if session.gesture_turn_locked() {
                    return true;
                }
                // turned anticlockwise is positive; the view turns clockwise on the screen
                session.view_gesture(p.0, p.1, 0.0, 0.0, 1.0, -delta.to_radians());
                if *phase == TouchPhase::Ended {
                    session.settle_turn();
                }
                true
            }
            WindowEvent::Touch(touch) => self.on_touch(session, touch, &over),
            _ => false,
        }
    }

    /// The pointer at `p`: while the view is being moved, moved by as much. True if it was.
    fn pan_to(&mut self, session: &CanvasSession, p: (f32, f32)) -> bool {
        let last = self.cursor.replace(p);
        let (true, Some(l)) = (self.panning, last) else { return false };
        if self.turns_model && !self.shift {
            session.turn_model(p.0 - l.0, p.1 - l.1);
        } else {
            session.view_gesture(p.0, p.1, p.0 - l.0, p.1 - l.1, 1.0, 0.0);
        }
        true
    }

    fn on_touch(&mut self, session: &CanvasSession, touch: &Touch, over: &impl Fn((f32, f32)) -> bool) -> bool {
        let p = (touch.location.x as f32, touch.location.y as f32);
        // a contact the system took back (a palm, say): what it did goes
        let phase = match touch.phase {
            TouchPhase::Ended if crate::pen::was_taken_back(touch.id) => TouchPhase::Cancelled,
            // a new contact isn't an old one's: that may have gone where nothing heard of it
            TouchPhase::Started => {
                crate::pen::was_taken_back(touch.id);
                TouchPhase::Started
            }
            phase => phase,
        };
        let pen = pen_pressure(touch);
        // a pen turned over rubs out with its eraser end
        let kind = match pen {
            Some(_) if crate::pen::eraser_end(touch.id) => PointerKind::StylusEraser,
            Some(_) => PointerKind::Stylus,
            None => PointerKind::Finger,
        };
        let pressure = pen.unwrap_or(1.0);
        // with keys held the pen moves the view, as the mouse does: Space moves it, Space and Ctrl
        // (⌘) zoom it, R turns it, and nothing is painted
        if pen.is_some() {
            match phase {
                TouchPhase::Started
                    if (self.space || self.r_key.is_some()) && self.down.is_empty() && !self.panning && !over(p) =>
                {
                    if !self.begin_key_drag(session, p) {
                        self.panning = true;
                        self.space_used = true;
                    }
                    self.pen_panning = Some(touch.id);
                    self.cursor = Some(p);
                    return false;
                }
                _ if self.pen_panning != Some(touch.id) => {}
                TouchPhase::Moved if self.key_drag.is_some() => {
                    self.cursor = Some(p);
                    session.key_drag_move(p.0, p.1, self.shift);
                    return true;
                }
                TouchPhase::Moved => return self.pan_to(session, p),
                TouchPhase::Ended | TouchPhase::Cancelled => {
                    self.end_key_drag(session);
                    self.panning = false;
                    self.pen_panning = None;
                    return false;
                }
                TouchPhase::Started => self.pen_panning = None,
            }
            // the pointer goes with it: a pen down moves no mouse (the wheel zooms about where it
            // is, and what shows once it lifts starts from there)
            self.cursor = Some(p);
        }
        let now = self.now_ms();
        let known = self.down.contains(&touch.id);
        match phase {
            TouchPhase::Started => {
                // a touch on the interface is the interface's, as on the tablet: a finger working
                // the side bar beside a hand resting on the canvas is no gesture with it (one kept
                // aside, so no outline hovers while it is down)
                if over(p) {
                    self.ignored.retain(|i| *i != touch.id);
                    self.ignored.push(touch.id);
                    return true;
                }
                if self.held_lifts.iter().any(|(id, _)| *id == touch.id) {
                    self.lift_held(session);
                }
                self.pressed(session, touch.id, kind, p, pressure);
                if pen.is_none() && self.hold_whole_touches {
                    self.fingers.retain(|f| f.id != touch.id);
                    self.fingers.push(FingerHeard { id: touch.id, idle_ms: 0.0, done_ms: self.now_ms() });
                    // lifts held back wait for the fingers still coming
                    if self.held_until.is_some() {
                        self.held_until = Some(now + HELD_LIFT_MS);
                    }
                }
                true
            }
            // a pen's moves wait for the frame; a finger's go on at once, for the gestures fingers
            // make together
            TouchPhase::Moved if known && pen.is_some() => {
                self.moved(touch.id, p, pressure);
                true
            }
            TouchPhase::Moved if known => {
                self.finger_heard(touch.id, now);
                let out = session.touch_move(touch.id, vec![self.sample(touch.id, p, pressure)]);
                self.take(out);
                self.finger_done(touch.id);
                true
            }
            TouchPhase::Ended if known => {
                let heard = self.finger_heard(touch.id, now);
                self.fingers.retain(|f| f.id != touch.id);
                // a finger that came whole, with none down but those held back: held back too
                let held = |id: &u64| self.held_lifts.iter().any(|(h, _)| h == id);
                let alone = self.down.iter().all(|id| *id == touch.id || held(id));
                if alone && heard.is_some_and(|f| f.idle_ms < WHOLE_TOUCH_MS) {
                    self.held_lifts.push((touch.id, p));
                    self.held_until = Some(now + HELD_LIFT_MS);
                    return true;
                }
                self.lift_held(session);
                self.lifted(session, touch.id, p, pressure, false);
                true
            }
            TouchPhase::Cancelled if known => {
                self.fingers.retain(|f| f.id != touch.id);
                self.lift_held(session);
                self.lifted(session, touch.id, p, pressure, true);
                true
            }
            TouchPhase::Ended | TouchPhase::Cancelled if self.ignored.contains(&touch.id) => {
                self.ignored.retain(|i| *i != touch.id);
                true
            }
            _ => false,
        }
    }
}

/// A touch's pressure if it is a pen's (none: a finger's), as the system says which it is (a touch
/// screen may give its fingers a pressure too), else a pressure meaning a pen. A pen landing or
/// lifting has no force, its pressure being nothing: its own is taken.
fn pen_pressure(touch: &Touch) -> Option<f32> {
    let force = touch.force.map(|f| f.normalized() as f32);
    match crate::pen::is_pen(touch.id) {
        Some(true) => Some(force.or_else(|| crate::pen::pressure(touch.id)).unwrap_or(1.0)),
        Some(false) => None,
        None => force,
    }
}

/// Whether `event` is the R key's: by its letter, or (a layout without Latin letters) by its
/// place, as the interface's own shortcuts take a key.
fn is_r(event: &KeyEvent) -> bool {
    match &event.logical_key {
        Key::Character(c) if c.eq_ignore_ascii_case("r") => true,
        Key::Character(c) => {
            egui::Key::from_name(c).is_none() && event.physical_key == PhysicalKey::Code(KeyCode::KeyR)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use winit::dpi::PhysicalPosition;
    use winit::event::{DeviceId, Force};

    use super::*;

    fn shown() -> Arc<CanvasSession> {
        let session = CanvasSession::new(400, 300);
        session.set_display_scale(1.0);
        session.show_in(800, 600);
        session
    }

    /// A pen's touch at (`x`, `y`), the system having said it is one.
    fn pen(id: u64, phase: TouchPhase, x: f64, y: f64) -> WindowEvent {
        crate::pen::pointer_kind(id, true);
        WindowEvent::Touch(Touch {
            device_id: DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(x, y),
            force: Some(Force::Normalized(0.5)),
            id,
        })
    }

    /// A finger's touch at (`x`, `y`), with a pressure as a laptop's screen may give it, the system
    /// having said it is a finger.
    fn finger(id: u64, phase: TouchPhase, x: f64, y: f64) -> WindowEvent {
        crate::pen::pointer_kind(id, false);
        WindowEvent::Touch(Touch {
            device_id: DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(x, y),
            force: Some(Force::Normalized(0.5)),
            id,
        })
    }

    fn send(input: &mut Input, session: &CanvasSession, event: &WindowEvent) {
        input.on_event(session, event, false, |_| false);
    }

    /// `id` pressed at (100, 100), moved and lifted at `to`.
    fn drag(input: &mut Input, session: &CanvasSession, id: u64, to: (f64, f64)) {
        send(input, session, &pen(id, TouchPhase::Started, 100.0, 100.0));
        send(input, session, &pen(id, TouchPhase::Moved, to.0, to.1));
        send(input, session, &pen(id, TouchPhase::Ended, to.0, to.1));
    }

    fn painted(input: &Input) -> bool {
        input.commands.iter().any(|c| matches!(c, TouchCommand::Cursor { .. }))
    }

    /// The view's zoom, its turn (radians) and where the canvas's corner shows.
    fn view(session: &CanvasSession) -> (f32, f32, (f32, f32)) {
        let m = session.view_matrix();
        (m[0].hypot(m[1]), m[1].atan2(m[0]), (m[4], m[5]))
    }

    #[test]
    fn a_finger_on_the_interface_takes_its_pointer_from_a_hand_resting_on_the_canvas() {
        let mut touches = InterfaceTouches::default();
        // the interface is left of x = 100
        let on_interface = |p: (f32, f32)| p.0 < 100.0;
        let phases = |heard: &[WindowEvent]| -> Vec<(u64, TouchPhase)> {
            heard
                .iter()
                .filter_map(|e| match e {
                    WindowEvent::Touch(t) => Some((t.id, t.phase)),
                    _ => None,
                })
                .collect()
        };
        let mut route = |event| phases(&touches.route(event, on_interface));
        // the hand rests on the canvas and moves a little
        assert_eq!(route(finger(1, TouchPhase::Started, 400.0, 300.0)), [(1, TouchPhase::Started)]);
        assert_eq!(route(finger(1, TouchPhase::Moved, 402.0, 301.0)), [(1, TouchPhase::Moved)]);
        // a finger lands on the side bar: the hand is cancelled for the interface first
        let landed = route(finger(2, TouchPhase::Started, 40.0, 300.0));
        assert_eq!(landed, [(1, TouchPhase::Cancelled), (2, TouchPhase::Started)]);
        // the finger drags a slider and lifts; the hand isn't heard from then on
        assert_eq!(route(finger(2, TouchPhase::Moved, 40.0, 250.0)), [(2, TouchPhase::Moved)]);
        assert_eq!(route(finger(1, TouchPhase::Moved, 404.0, 302.0)), []);
        assert_eq!(route(finger(2, TouchPhase::Ended, 40.0, 250.0)), [(2, TouchPhase::Ended)]);
        assert_eq!(route(finger(1, TouchPhase::Ended, 404.0, 302.0)), []);
        // a pinch on the canvas is as it was: its second finger goes on
        assert_eq!(route(finger(3, TouchPhase::Started, 400.0, 300.0)), [(3, TouchPhase::Started)]);
        assert_eq!(route(finger(4, TouchPhase::Started, 500.0, 300.0)), [(4, TouchPhase::Started)]);
        assert_eq!(route(finger(4, TouchPhase::Ended, 500.0, 300.0)), [(4, TouchPhase::Ended)]);
        assert_eq!(route(finger(3, TouchPhase::Ended, 400.0, 300.0)), [(3, TouchPhase::Ended)]);
        // and a finger on the interface first keeps it
        assert_eq!(route(finger(5, TouchPhase::Started, 40.0, 300.0)), [(5, TouchPhase::Started)]);
        assert_eq!(route(finger(6, TouchPhase::Started, 60.0, 300.0)), [(6, TouchPhase::Started)]);
    }

    #[test]
    fn a_pen_moving_the_mouse_pointer_tells_the_interface_once_that_it_is_on_a_touch_screen() {
        let (ctx, mut told) = (egui::Context::default(), PenTouchScreen::default());
        let mut frame = |pen_moves_mouse: bool| {
            let mut raw = egui::RawInput::default();
            told.tell(&mut raw, pen_moves_mouse);
            let touches = raw.events.iter().filter(|e| matches!(e, egui::Event::Touch { .. })).count();
            ctx.run_ui(raw, |_| {}).textures_delta.clear();
            (touches, ctx.input(|i| (i.has_touch_screen(), i.any_touches())))
        };
        // the mouse alone: no touch screen
        assert_eq!(frame(false), (0, (false, false)));
        // a tablet's pen moves the pointer: told once, with no touch left down
        assert_eq!(frame(true), (1, (true, false)));
        assert_eq!(frame(true), (0, (true, false)));
        assert_eq!(frame(false), (0, (true, false)));
    }

    #[test]
    fn a_finger_on_the_interface_beside_a_resting_hand_is_no_gesture_on_the_canvas() {
        let (session, mut input) = (shown(), Input::default());
        session.set_touch_settings(procraft_core::TouchSettings::default());
        let on_interface = |p: (f32, f32)| p.0 < 100.0;
        let mut send = |event: WindowEvent| input.on_event(&session, &event, false, on_interface);
        send(finger(1, TouchPhase::Started, 400.0, 300.0));
        // a quick tap on the side bar: no two-finger tap (an undo) with the hand
        assert!(send(finger(2, TouchPhase::Started, 40.0, 300.0)));
        assert!(send(finger(2, TouchPhase::Ended, 40.0, 300.0)));
        send(finger(1, TouchPhase::Ended, 400.0, 300.0));
        assert!(!input.commands.iter().any(|c| matches!(c, TouchCommand::Undo)), "{:?}", input.commands);
    }

    #[test]
    fn two_fingers_tapped_right_after_the_mouse_or_pen_undo_at_once() {
        let (session, mut input) = (shown(), Input::default());
        session.set_touch_settings(procraft_core::TouchSettings::default());
        let moved = |x: f64, y: f64| WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(x, y),
        };
        let button = |state| WindowEvent::MouseInput { device_id: DeviceId::dummy(), state, button: MouseButton::Left };
        // a tablet's pen as X11 gives it: the mouse, pressed, moved and let go
        send(&mut input, &session, &moved(100.0, 100.0));
        send(&mut input, &session, &button(ElementState::Pressed));
        send(&mut input, &session, &moved(200.0, 150.0));
        send(&mut input, &session, &button(ElementState::Released));
        input.commands.clear();
        let tap = |input: &mut Input, ids: (u64, u64)| {
            send(input, &session, &finger(ids.0, TouchPhase::Started, 300.0, 300.0));
            send(input, &session, &finger(ids.1, TouchPhase::Started, 400.0, 300.0));
            send(input, &session, &finger(ids.0, TouchPhase::Ended, 300.0, 300.0));
            send(input, &session, &finger(ids.1, TouchPhase::Ended, 400.0, 300.0));
        };
        tap(&mut input, (9001, 9002));
        assert!(input.commands.contains(&TouchCommand::Undo), "after the mouse: {:?}", input.commands);
        // and after the pen as a pen (Windows, Wayland)
        drag(&mut input, &session, 7, (200.0, 150.0));
        input.commands.clear();
        tap(&mut input, (9003, 9004));
        assert!(input.commands.contains(&TouchCommand::Undo), "after the pen: {:?}", input.commands);
    }

    /// A finger's touch as winit gives it under X11 at `ms`: no force, the system saying nothing of
    /// what it is, and the first finger down (`first`) moving the pointer just before it.
    fn x11_finger(
        input: &mut Input,
        session: &CanvasSession,
        id: u64,
        phase: TouchPhase,
        at: (f64, f64),
        first: bool,
        ms: f64,
    ) {
        input.test_clock = Some(ms);
        let location = PhysicalPosition::new(at.0, at.1);
        if first && phase != TouchPhase::Ended {
            send(input, session, &WindowEvent::CursorMoved { device_id: DeviceId::dummy(), position: location });
        }
        send(
            input,
            session,
            &WindowEvent::Touch(Touch { device_id: DeviceId::dummy(), phase, location, force: None, id }),
        );
    }

    /// A finger's whole touch handed on at once at `ms`, as an X11 window manager that held it
    /// back does once it lifted: landing at `at`, moving half a pixel, lifting.
    fn x11_whole_tap(input: &mut Input, session: &CanvasSession, id: u64, at: (f64, f64), ms: f64) {
        x11_finger(input, session, id, TouchPhase::Started, at, true, ms);
        x11_finger(input, session, id, TouchPhase::Moved, (at.0 + 0.5, at.1), true, ms);
        x11_finger(input, session, id, TouchPhase::Ended, (at.0 + 0.5, at.1), true, ms);
    }

    #[test]
    fn fingers_tapped_together_that_x11_hands_on_one_after_the_other_still_undo_and_redo() {
        let x11 = |hold| {
            let input = Input { hold_whole_touches: hold, test_clock: Some(0.0), ..Input::default() };
            let session = shown();
            session.set_touch_settings(procraft_core::TouchSettings::default());
            (session, input)
        };
        let undid = |input: &Input| input.commands.contains(&TouchCommand::Undo);
        // two fingers tapped together, each handed on whole as it lifted, 40 ms apart (Cinnamon
        // holds each until it lifts): as the window had them, two taps of one finger each
        let (session, mut input) = x11(false);
        x11_whole_tap(&mut input, &session, 3101, (300.0, 300.0), 1000.0);
        x11_whole_tap(&mut input, &session, 3102, (400.0, 300.0), 1040.0);
        input.test_clock = Some(1040.0 + HELD_LIFT_MS);
        input.tick(&session);
        assert!(!undid(&input), "{:?}", input.commands);
        // held back, they lift together: an undo, once no more fingers come
        let (session, mut input) = x11(true);
        x11_whole_tap(&mut input, &session, 3103, (300.0, 300.0), 1000.0);
        x11_whole_tap(&mut input, &session, 3104, (400.0, 300.0), 1040.0);
        assert!(!undid(&input));
        input.test_clock = Some(1040.0 + HELD_LIFT_MS - 1.0);
        assert!(!input.tick(&session));
        assert!(input.next_deadline(&session).is_some());
        input.test_clock = Some(1040.0 + HELD_LIFT_MS);
        assert!(input.tick(&session));
        assert!(undid(&input), "{:?}", input.commands);
        assert!(input.idle());
        // three that way: a redo
        input.commands.clear();
        for (k, id) in [3105, 3106, 3107].into_iter().enumerate() {
            x11_whole_tap(&mut input, &session, id, (300.0 + 100.0 * k as f64, 300.0), 2000.0 + 30.0 * k as f64);
        }
        input.test_clock = Some(2060.0 + HELD_LIFT_MS);
        assert!(input.tick(&session));
        assert!(input.commands.contains(&TouchCommand::Redo), "{:?}", input.commands);
        assert!(!undid(&input), "{:?}", input.commands);
    }

    #[test]
    fn x11_fingers_heard_as_they_go_lift_at_once_and_pinch_and_a_lone_whole_tap_lifts_a_moment_later() {
        let (session, mut input) =
            (shown(), Input { hold_whole_touches: true, test_clock: Some(0.0), ..Input::default() });
        session.set_touch_settings(procraft_core::TouchSettings::default());
        // a two-finger tap heard as it went: the undo at the first lift, nothing held
        x11_finger(&mut input, &session, 3201, TouchPhase::Started, (300.0, 300.0), true, 1000.0);
        x11_finger(&mut input, &session, 3202, TouchPhase::Started, (400.0, 300.0), false, 1020.0);
        x11_finger(&mut input, &session, 3201, TouchPhase::Ended, (300.0, 300.0), true, 1090.0);
        x11_finger(&mut input, &session, 3202, TouchPhase::Ended, (400.0, 300.0), false, 1100.0);
        assert!(input.commands.contains(&TouchCommand::Undo), "{:?}", input.commands);
        assert_eq!(input.next_deadline(&session), None);
        assert!(input.idle());
        // a pinch whose first finger's moves came held back, all at once, then the second's, then
        // both as they went: the canvas zooms, and both lift as they do
        input.commands.clear();
        let before = view(&session).0;
        x11_finger(&mut input, &session, 3203, TouchPhase::Started, (300.0, 300.0), true, 3000.0);
        for k in 1..=5 {
            x11_finger(
                &mut input,
                &session,
                3203,
                TouchPhase::Moved,
                (300.0 - 6.0 * f64::from(k), 300.0),
                true,
                3000.0,
            );
        }
        x11_finger(&mut input, &session, 3204, TouchPhase::Started, (500.0, 300.0), false, 3040.0);
        for k in 1..=5 {
            x11_finger(
                &mut input,
                &session,
                3204,
                TouchPhase::Moved,
                (500.0 + 6.0 * f64::from(k), 300.0),
                false,
                3040.0,
            );
        }
        for k in 1..=10 {
            let (ms, out) = (3040.0 + 10.0 * f64::from(k), 30.0 + 6.0 * f64::from(k));
            x11_finger(&mut input, &session, 3203, TouchPhase::Moved, (300.0 - out, 300.0), true, ms);
            x11_finger(&mut input, &session, 3204, TouchPhase::Moved, (500.0 + out, 300.0), false, ms);
        }
        x11_finger(&mut input, &session, 3203, TouchPhase::Ended, (210.0, 300.0), true, 3150.0);
        x11_finger(&mut input, &session, 3204, TouchPhase::Ended, (590.0, 300.0), false, 3155.0);
        assert!(view(&session).0 > before * 1.5, "{} from {before}", view(&session).0);
        assert!(input.idle() && input.held_lifts.is_empty());
        assert!(!input.commands.contains(&TouchCommand::StrokeEnded), "{:?}", input.commands);
        // one finger's whole tap: its dot's stroke ends once no other has come
        input.commands.clear();
        x11_whole_tap(&mut input, &session, 3205, (300.0, 300.0), 5000.0);
        assert!(!input.commands.contains(&TouchCommand::StrokeEnded), "{:?}", input.commands);
        input.test_clock = Some(5000.0 + HELD_LIFT_MS);
        assert!(input.tick(&session));
        assert!(input.commands.contains(&TouchCommand::StrokeEnded), "{:?}", input.commands);
        assert!(!input.commands.contains(&TouchCommand::Undo), "{:?}", input.commands);
    }

    #[test]
    fn a_pinch_with_finger_painting_on_leaves_no_stroke_and_zooms_however_hard_the_fingers_press() {
        let (session, mut input) = (shown(), Input::default());
        session.set_touch_settings(procraft_core::TouchSettings::default());
        let before = view(&session).0;
        // the first paints until the second lands, each move its own event, as the system's frames
        // give them
        send(&mut input, &session, &finger(9001, TouchPhase::Started, 300.0, 300.0));
        send(&mut input, &session, &finger(9001, TouchPhase::Moved, 302.0, 301.0));
        assert!(painted(&input), "a finger paints as it lands");
        send(&mut input, &session, &finger(9002, TouchPhase::Started, 500.0, 300.0));
        for step in 1..=10 {
            let out = f64::from(step) * 8.0;
            send(&mut input, &session, &finger(9001, TouchPhase::Moved, 302.0 - out, 300.0));
            send(&mut input, &session, &finger(9002, TouchPhase::Moved, 500.0 + out, 300.0));
        }
        send(&mut input, &session, &finger(9001, TouchPhase::Ended, 222.0, 300.0));
        send(&mut input, &session, &finger(9002, TouchPhase::Ended, 580.0, 300.0));
        assert!(view(&session).0 > before * 1.5, "{} from {before}", view(&session).0);
        assert!(!input.commands.contains(&TouchCommand::StrokeEnded), "{:?}", input.commands);
    }

    #[test]
    fn a_finger_the_system_takes_back_leaves_no_stroke() {
        let (session, mut input) = (shown(), Input::default());
        session.set_touch_settings(procraft_core::TouchSettings::default());
        send(&mut input, &session, &finger(9003, TouchPhase::Started, 300.0, 300.0));
        send(&mut input, &session, &finger(9003, TouchPhase::Moved, 340.0, 320.0));
        assert!(painted(&input), "a finger paints as it lands");
        crate::pen::taken_back(9003);
        send(&mut input, &session, &finger(9003, TouchPhase::Ended, 340.0, 320.0));
        assert!(!input.commands.contains(&TouchCommand::StrokeEnded), "{:?}", input.commands);
        // and only that one: the next finger paints and keeps its stroke
        input.commands.clear();
        send(&mut input, &session, &finger(9004, TouchPhase::Started, 300.0, 300.0));
        send(&mut input, &session, &finger(9004, TouchPhase::Moved, 340.0, 320.0));
        send(&mut input, &session, &finger(9004, TouchPhase::Ended, 340.0, 320.0));
        assert!(input.commands.contains(&TouchCommand::StrokeEnded), "{:?}", input.commands);
    }

    #[test]
    fn a_pen_dragged_with_space_and_ctrl_zooms_the_canvas_as_the_mouse_does_and_paints_nothing() {
        let (session, mut input) = (shown(), Input::default());
        (input.space, input.control) = (true, true);
        let (before, ..) = view(&session);
        drag(&mut input, &session, 7001, (280.0, 100.0));
        let (after, ..) = view(&session);
        // 180 right is e times as near
        assert!((after / before - std::f32::consts::E).abs() < 1e-3, "{before} {after}");
        assert!(!painted(&input) && input.idle(), "{:?}", input.commands);
        // nor is Space's tap left to open the QuickMenu
        input.space = false;
        assert!(input.space_used);
    }

    #[test]
    fn a_pen_dragged_with_r_held_turns_the_canvas_as_the_mouse_does_and_paints_nothing() {
        let (session, mut input) = (shown(), Input::default());
        input.r_key = Some(Instant::now());
        send(&mut input, &session, &pen(7002, TouchPhase::Started, 600.0, 300.0));
        send(&mut input, &session, &pen(7002, TouchPhase::Moved, 400.0, 500.0));
        assert!(input.moving_view() && !input.pressing());
        send(&mut input, &session, &pen(7002, TouchPhase::Ended, 400.0, 500.0));
        // a quarter of the way round the window's middle
        let (_, turn, _) = view(&session);
        assert!((turn - std::f32::consts::FRAC_PI_2).abs() < 1e-3, "{turn}");
        assert!(!painted(&input) && input.r_dragged);
    }

    #[test]
    fn a_pen_dragged_with_space_alone_still_moves_the_canvas() {
        let (session, mut input) = (shown(), Input::default());
        input.space = true;
        let (before, _, (x, y)) = view(&session);
        drag(&mut input, &session, 7003, (130.0, 140.0));
        let (after, _, moved) = view(&session);
        assert_eq!(before, after);
        assert!((moved.0 - x - 30.0).abs() < 1e-3 && (moved.1 - y - 40.0).abs() < 1e-3, "{moved:?}");
        assert!(!painted(&input) && input.idle());
    }

    #[test]
    fn a_zoom_let_go_with_the_pen_down_stops_and_paints_nothing_after() {
        let (session, mut input) = (shown(), Input::default());
        (input.space, input.control) = (true, true);
        send(&mut input, &session, &pen(7004, TouchPhase::Started, 100.0, 100.0));
        // Space let go, as its key's event ends the zoom
        input.space = false;
        input.end_key_drag(&session);
        let zoom = view(&session).0;
        send(&mut input, &session, &pen(7004, TouchPhase::Moved, 300.0, 100.0));
        send(&mut input, &session, &pen(7004, TouchPhase::Ended, 300.0, 100.0));
        assert_eq!(view(&session).0, zoom);
        assert!(!painted(&input) && input.idle());
        // the next stroke paints
        drag(&mut input, &session, 7004, (120.0, 100.0));
        assert!(painted(&input));
    }

    #[test]
    fn a_pen_held_as_the_window_loses_the_keys_moves_the_view_no_more() {
        for keys in [(true, true), (true, false)] {
            let (session, mut input) = (shown(), Input::default());
            (input.space, input.control) = keys;
            send(&mut input, &session, &pen(7005, TouchPhase::Started, 100.0, 100.0));
            send(&mut input, &session, &WindowEvent::Focused(false));
            assert!(input.idle() && !input.moving_view());
            // its lift never came: the next stroke paints all the same, moves and all
            send(&mut input, &session, &pen(7005, TouchPhase::Started, 100.0, 100.0));
            input.commands.clear();
            send(&mut input, &session, &pen(7005, TouchPhase::Moved, 140.0, 100.0));
            // (handed on with the frame)
            input.flush(&session);
            assert!(painted(&input), "{keys:?}");
        }
    }

    #[test]
    fn the_wheel_zooms_about_where_the_pen_last_was_though_the_mouse_never_came() {
        let (session, mut input) = (shown(), Input::default());
        drag(&mut input, &session, 7006, (300.0, 200.0));
        let (before, ..) = view(&session);
        let wheel = WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta: MouseScrollDelta::LineDelta(0.0, 1.0),
            phase: TouchPhase::Moved,
        };
        assert!(input.on_event(&session, &wheel, false, |_| false));
        assert!((view(&session).0 / before - WHEEL_ZOOM).abs() < 1e-3);
        assert_eq!(input.cursor(), Some((300.0, 200.0)));
    }

    #[test]
    fn fingers_leave_the_pointer_alone() {
        let (session, mut input) = (shown(), Input::default());
        crate::pen::pointer_kind(7007, false);
        let finger = |phase, x, y| {
            let location = PhysicalPosition::new(x, y);
            WindowEvent::Touch(Touch { device_id: DeviceId::dummy(), phase, location, force: None, id: 7007 })
        };
        send(&mut input, &session, &finger(TouchPhase::Started, 100.0, 100.0));
        send(&mut input, &session, &finger(TouchPhase::Moved, 140.0, 100.0));
        assert_eq!(input.cursor(), None);
    }

    thread_local! {
        /// The tablet pen this test's mouse pointer reads, if any.
        static TABLET_PEN: std::cell::Cell<Option<crate::pen::MousePen>> = const { std::cell::Cell::new(None) };
    }

    fn tablet_pen(_: (f32, f32)) -> Option<crate::pen::MousePen> {
        TABLET_PEN.with(std::cell::Cell::get)
    }

    #[test]
    fn a_stroke_the_mouse_pointer_began_is_the_tablet_pens_once_the_pens_reading_comes_just_after() {
        let device_id = DeviceId::dummy();
        let moved = |x: f64| WindowEvent::CursorMoved { device_id, position: PhysicalPosition::new(x, 100.0) };
        let press = |state| WindowEvent::MouseInput { device_id, state, button: MouseButton::Left };
        // the pointer pressed at (100, 100) with no tablet pen read, moved `moves` times before the
        // pen's reading comes (its tip not yet down, as its packets run ahead of the mouse), and
        // moved on: whether the stroke became the pen's, which hovers there until it presses
        let pen_took_it = |moves: Option<usize>| {
            let (session, mut input) = (shown(), Input { read_mouse_pen: tablet_pen, ..Input::default() });
            // a pen has drawn with a pressure: pens are known to give one
            drag(&mut input, &session, 7020, (300.0, 300.0));
            input.commands.clear();
            TABLET_PEN.with(|p| p.set(None));
            send(&mut input, &session, &moved(100.0));
            send(&mut input, &session, &press(ElementState::Pressed));
            assert!(painted(&input), "the mouse's stroke begins");
            for i in 0..8 {
                if Some(i) == moves {
                    TABLET_PEN.with(|p| p.set(Some(crate::pen::MousePen { pressure: 0.0, eraser: false })));
                }
                send(&mut input, &session, &moved(102.0 + i as f64));
            }
            send(&mut input, &session, &press(ElementState::Released));
            input.commands.iter().any(|c| matches!(c, TouchCommand::PenHovers { x, y } if (*x, *y) == (100.0, 100.0)))
        };
        // the reading came with the first move, or a few later
        assert!(pen_took_it(Some(0)));
        assert!(pen_took_it(Some(3)));
        // too long after: the stroke stays as it began; and the mouse alone stays the mouse
        assert!(!pen_took_it(Some(6)));
        assert!(!pen_took_it(None));
    }

    #[test]
    fn what_paints_from_afar_is_what_isnt_under_the_hand() {
        let (session, mut input) = (shown(), Input::default());
        // the mouse
        let (device_id, position) = (DeviceId::dummy(), PhysicalPosition::new(100.0, 100.0));
        send(&mut input, &session, &WindowEvent::CursorMoved { device_id, position });
        let press = |state| WindowEvent::MouseInput { device_id, state, button: MouseButton::Left };
        assert!(!input.pressing_from_afar(), "nothing is pressed");
        send(&mut input, &session, &press(ElementState::Pressed));
        assert!(input.pressing_from_afar() && painted(&input));
        send(&mut input, &session, &press(ElementState::Released));
        assert!(!input.pressing_from_afar());
        // a tablet's pen, one the system can't say of, and one in a screen
        crate::pen::pen_on_screen(7008, false);
        crate::pen::pen_on_screen(7010, true);
        for (id, afar) in [(7008, true), (7009, true), (7010, false)] {
            send(&mut input, &session, &pen(id, TouchPhase::Started, 100.0, 100.0));
            assert_eq!(input.pressing_from_afar(), afar, "{id}");
            send(&mut input, &session, &pen(id, TouchPhase::Ended, 100.0, 100.0));
        }
        // a finger
        crate::pen::pointer_kind(7011, false);
        let location = PhysicalPosition::new(100.0, 100.0);
        let phase = TouchPhase::Started;
        let finger = WindowEvent::Touch(Touch { device_id, phase, location, force: None, id: 7011 });
        send(&mut input, &session, &finger);
        assert!(input.pressing() && !input.pressing_from_afar());
    }

    /// A window on a headless GPU showing a `side` square canvas fitted in it, at the report's scale
    /// (1.2); none without a GPU.
    struct Window {
        session: Arc<CanvasSession>,
        gpu: procraft_gpu::Gpu,
        target: procraft_gpu::wgpu::Texture,
        size: (u32, u32),
        started: Instant,
    }

    impl Window {
        fn open(side: u32) -> Option<Window> {
            use procraft_gpu::wgpu;
            let gpu = procraft_gpu::Gpu::headless().map_err(|e| eprintln!("skipped: {e}")).ok()?;
            let size = (1728, 1080);
            let session = CanvasSession::new(side, side);
            session.set_display_scale(1.2);
            session.use_gpu(gpu.clone());
            session.show_in(size.0, size.1);
            let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("window"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let window = Window { session, gpu, target, size, started: Instant::now() };
            window.settle();
            Some(window)
        }

        /// A frame drawn, the GPU waited for; whether another is wanted at once.
        fn frame(&self) -> bool {
            let view = self.target.create_view(&Default::default());
            let format = procraft_gpu::wgpu::TextureFormat::Rgba8Unorm;
            let again =
                self.session.draw(&view, format, self.size, procraft_engine::profile::Profile::Srgb, self.started);
            self.gpu.flush();
            again == Some(Duration::ZERO)
        }

        /// Frames drawn until the canvas wants no more at once.
        fn settle(&self) {
            for _ in 0..100 {
                if !self.frame() {
                    break;
                }
            }
        }

        /// What the window shows, the canvas settled.
        fn picture(&self) -> Vec<u8> {
            self.settle();
            procraft_gpu::read_texture(&self.gpu, &self.target, self.size.0, self.size.1).unwrap()
        }

        /// The library's brush named `name` chosen.
        fn brush(&self, name: &str) -> bool {
            let library = self.session.brush_library();
            let found = library
                .iter()
                .enumerate()
                .find_map(|(set, s)| s.brushes.iter().position(|b| b.name == name).map(|i| (set as u32, i as u32)));
            found.is_some_and(|(set, i)| self.session.select_brush(set, i))
        }
    }

    /// What a stroke through the input cost and handed the canvas.
    struct Stroke {
        /// The pointer's moves (winit's `CursorMoved`s), and what the canvas was handed of them.
        moves: usize,
        handed: Handed,
        /// Each move's handling, and each frame's (the moves waiting handed over, then drawn).
        per_move: Vec<Duration>,
        per_frame: Vec<Duration>,
        picture: Vec<u8>,
    }

    impl Stroke {
        /// The main thread's time for the stroke's second: every move's and every frame's.
        fn busy(&self) -> Duration {
            self.per_move.iter().chain(&self.per_frame).sum()
        }

        fn median(times: &[Duration]) -> Duration {
            let mut sorted = times.to_vec();
            sorted.sort();
            sorted.get(sorted.len() / 2).copied().unwrap_or_default()
        }

        fn said(&self, what: &str) -> String {
            format!(
                "{what:<27} {:4} moves, {:4} samples handed in {:4} batches, {:2} frames; a move {:6.1} us, \
                 a frame {:5.2} ms (medians); busy {:4.0} ms for the second",
                self.moves,
                self.handed.samples,
                self.handed.batches,
                self.per_frame.len(),
                Stroke::median(&self.per_move).as_secs_f64() * 1e6,
                Stroke::median(&self.per_frame).as_secs_f64() * 1e3,
                self.busy().as_secs_f64() * 1e3,
            )
        }
    }

    /// A second's stroke at 1000 moves a second through the desktop's input on `window`, as X11
    /// gives one: the mouse pointer, moved by a tablet's pen (`pen`: finer than a pixel, read with
    /// its pressure and lean, each move followed by the events X11 makes of its other axes) or by
    /// the mouse (in whole pixels, a move only on reaching another). A frame each 1/60 s hands the
    /// canvas the moves waiting, as the interface does, and draws it; with `each_move` they are
    /// handed on as each comes, as before they waited. The clock is the test's own, a move a
    /// millisecond, so a stroke drawn twice is drawn alike. Undone after.
    fn stroke_at_1000_hz(window: &Window, pen: bool, each_move: bool) -> Stroke {
        let session = &window.session;
        let mut input = Input { read_mouse_pen: tablet_pen, test_clock: Some(0.0), ..Input::default() };
        let device_id = DeviceId::dummy();
        let reading =
            |k: usize| crate::pen::MousePen { pressure: 0.25 + 0.6 * (k as f32 / 170.0).sin().abs(), eraser: false };
        let read = |k: usize| {
            let pen = pen.then(|| reading(k));
            TABLET_PEN.with(|p| p.set(pen));
            let lean = crate::pen::lean_of(25.0 * (k as f32 / 300.0).sin(), 20.0);
            crate::pen::set_mouse_pen(pen.map(|p| (p, Some(lean))));
        };
        // across the canvas, waving up and down, at about 1500 window pixels a second
        let at = |k: usize| {
            let t = k as f64 / 1000.0;
            let (x, y) = (300.0 + 1100.0 * t, 540.0 + 250.0 * (t * 6.0).sin());
            if pen { (x, y) } else { (x.round(), y.round()) }
        };
        let moved = |(x, y): (f64, f64)| WindowEvent::CursorMoved { device_id, position: PhysicalPosition::new(x, y) };
        let press = |state| WindowEvent::MouseInput { device_id, state, button: MouseButton::Left };
        read(0);
        send(&mut input, session, &moved(at(0)));
        send(&mut input, session, &press(ElementState::Pressed));
        let (mut per_move, mut per_frame, mut last) = (Vec::new(), Vec::new(), at(0));
        for k in 1..=1000 {
            input.test_clock = Some(k as f64);
            read(k);
            let p = at(k);
            if p != last {
                last = p;
                let took = Instant::now();
                send(&mut input, session, &moved(p));
                // its pressure and tilts (X11's axes 2 to 4), which nothing reads
                if pen {
                    for axis in 2..5 {
                        send(&mut input, session, &WindowEvent::AxisMotion { device_id, axis, value: k as f64 });
                    }
                }
                if each_move {
                    input.flush(session);
                }
                per_move.push(took.elapsed());
            }
            if k * 60 / 1000 != (k - 1) * 60 / 1000 {
                let took = Instant::now();
                input.flush(session);
                window.frame();
                per_frame.push(took.elapsed());
            }
        }
        input.test_clock = Some(1001.0);
        send(&mut input, session, &press(ElementState::Released));
        let picture = window.picture();
        let handed = input.handed();
        crate::pen::set_mouse_pen(None);
        session.undo();
        window.settle();
        Stroke { moves: per_move.len(), handed, per_move, per_frame, picture }
    }

    /// How far apart two pictures are at most, in any channel of any pixel.
    fn most_apart(a: &[u8], b: &[u8]) -> u8 {
        assert_eq!(a.len(), b.len());
        a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0)
    }

    /// A tablet's pen as X11 gives it, at 1000 moves a second with its pressure and lean: every
    /// sample reaches the stroke, the canvas is handed them at most once a frame (and once more as
    /// it lifts), and it lays just what it laid when each move went on as it came. A move then
    /// costs a small share of what it cost (a stroke's step and tip), and the pen's second costs
    /// the main thread about what the mouse's does. The mouse, drawn alike, lays what it did too.
    /// The brush is one that shows its tip, the costly kind. The four strokes are drawn in turn
    /// `ROUNDS` times, so a busy machine slows each alike, and each is timed by its quickest round
    /// (other work only adds); what each costs is printed. `pen_cost_by_brush` holds a move to
    /// `MOVE_BUDGET_US`.
    #[test]
    fn a_1000_hz_pen_reaches_the_stroke_whole_once_a_frame() {
        let _one = crate::pen::tests::mouse_pointer_tests();
        let Some(window) = Window::open(2048) else { return };
        assert!(window.brush("Studio Pen"));
        let blank = window.picture();
        // each round's main-thread time for the second and median move: a pen's strokes each move
        // and once a frame, then the mouse's
        let mut timed = [(); 4].map(|()| Vec::new());
        for round in 1..=ROUNDS {
            for (k, (pen, what)) in [(true, "a tablet's pen"), (false, "the mouse")].into_iter().enumerate() {
                let each = stroke_at_1000_hz(&window, pen, true);
                let waiting = stroke_at_1000_hz(&window, pen, false);
                println!("{}", each.said(&format!("{round}: {what}, each move")));
                println!("{}", waiting.said(&format!("{round}: {what}, once a frame")));
                assert!(most_apart(&blank, &waiting.picture) > 100, "{what}: nothing was drawn");
                let apart = most_apart(&each.picture, &waiting.picture);
                assert!(apart <= 2, "{what}: once a frame, the stroke came out otherwise ({apart} apart)");
                // every move reached the stroke, at most once a frame and once more as it lifted
                assert_eq!(each.handed, Handed { batches: each.moves, samples: each.moves }, "{what}");
                assert_eq!(waiting.handed.samples, waiting.moves, "{what}: samples were lost");
                assert!(waiting.handed.batches <= waiting.per_frame.len() + 1, "{what}: {:?}", waiting.handed);
                for (i, stroke) in [(2 * k, each), (2 * k + 1, waiting)] {
                    timed[i].push((stroke.busy(), Stroke::median(&stroke.per_move)));
                }
            }
        }
        let quickest = |times: &[(Duration, Duration)]| {
            let busy = times.iter().map(|t| t.0).min().unwrap_or_default();
            (busy, times.iter().map(|t| t.1).min().unwrap_or_default())
        };
        let [(each_busy, each_move), (pen_busy, a_move), _, (mouse_busy, _)] = timed.map(|t| quickest(&t));
        println!(
            "quickest: a pen's move {a_move:?} once a frame, {each_move:?} each move; its second {pen_busy:?} \
             once a frame, {each_busy:?} each move; the mouse's {mouse_busy:?} once a frame"
        );
        // a move waiting costs a small share of one handed on: no stroke's step, no tip, no wait on
        // the canvas
        assert!(a_move * MOVE_SHARE <= each_move, "a pen's move took {a_move:?} once a frame, {each_move:?} each move");
        // the pen's second costs at most half what it did, and about what the mouse's does
        assert!(pen_busy * 2 <= each_busy, "{pen_busy:?} once a frame, {each_busy:?} each move");
        assert!(
            pen_busy <= mouse_busy * 2 + Duration::from_millis(50),
            "the pen {pen_busy:?}, the mouse {mouse_busy:?}"
        );
    }

    /// The rounds of `a_1000_hz_pen_reaches_the_stroke_whole_once_a_frame`.
    const ROUNDS: usize = 3;

    /// How many times less a pen's move costs the main thread while it waits for the frame than
    /// one handed on at once (a stroke's step and its tip).
    const MOVE_SHARE: u32 = 10;

    /// What a pen's move may cost the main thread while it waits for the frame (microseconds; one
    /// handed on at once cost hundreds, a tip's brush a thousand, in a test build).
    const MOVE_BUDGET_US: u64 = 50;

    /// What a stroke through the desktop's input costs with each brush in `PEN_COST_BRUSHES`
    /// (names, `,` between; "Studio Pen,Gouache,Soft Airbrush,Technical Pencil"), on a canvas
    /// `PEN_COST_SIDE` (3600) square: a tablet's pen and the mouse under X11, at 1000 moves a
    /// second, each move handed on as it came and once a frame. Fails if a pen's move waiting for
    /// the frame costs past `MOVE_BUDGET_US` (the median); run alone (other work on the machine
    /// slows it).
    /// `cargo test -p procraft-desktop pen_cost_by_brush -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn pen_cost_by_brush() {
        let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
        let _one = crate::pen::tests::mouse_pointer_tests();
        let side = env("PEN_COST_SIDE", "3600").parse().unwrap_or(3600);
        let Some(window) = Window::open(side) else { return };
        let info = window.gpu.adapter.get_info();
        println!("{} ({:?}), canvas {side} square", info.name, info.backend);
        let mut slow = Vec::new();
        for name in env("PEN_COST_BRUSHES", "Studio Pen,Gouache,Soft Airbrush,Technical Pencil").split(',') {
            if !window.brush(name) {
                println!("no {name}");
                continue;
            }
            for (pen, what) in [(true, "pen"), (false, "mouse")] {
                for (each_move, how) in [(true, "each move"), (false, "once a frame")] {
                    let stroke = stroke_at_1000_hz(&window, pen, each_move);
                    println!("{}", stroke.said(&format!("{name}, {what}, {how}")));
                    let a_move = Stroke::median(&stroke.per_move);
                    if pen && !each_move && a_move > Duration::from_micros(MOVE_BUDGET_US) {
                        slow.push(format!("{name} {a_move:?}"));
                    }
                }
            }
        }
        assert!(slow.is_empty(), "a pen's move past {MOVE_BUDGET_US} us waiting for the frame: {slow:?}");
    }

    /// A pen's touches (Windows Ink's, the tablet protocol's on Wayland) wait for the frame as its
    /// moves of the mouse pointer do; a finger's go on at once, for the gestures fingers make
    /// together.
    #[test]
    fn a_pens_touch_moves_wait_for_the_frame_and_a_fingers_go_on_at_once() {
        let (session, mut input) = (shown(), Input::default());
        send(&mut input, &session, &pen(7030, TouchPhase::Started, 100.0, 100.0));
        input.commands.clear();
        for x in [110.0, 120.0, 130.0] {
            send(&mut input, &session, &pen(7030, TouchPhase::Moved, x, 100.0));
        }
        assert!(!painted(&input) && input.handed() == Handed::default());
        // the frame hands them on together
        assert!(input.flush(&session));
        assert!(painted(&input));
        assert_eq!(input.handed(), Handed { batches: 1, samples: 3 });
        // and the lift comes after them, as anything else does
        send(&mut input, &session, &pen(7030, TouchPhase::Moved, 140.0, 100.0));
        send(&mut input, &session, &pen(7030, TouchPhase::Ended, 140.0, 100.0));
        assert_eq!(input.handed(), Handed { batches: 2, samples: 4 });
        assert!(input.commands.contains(&TouchCommand::StrokeEnded), "{:?}", input.commands);
        // a finger painting (on a canvas no pen has been near, which would take it for a hand)
        let (session, mut input) = (shown(), Input::default());
        session.set_touch_settings(procraft_core::TouchSettings::default());
        send(&mut input, &session, &finger(9030, TouchPhase::Started, 300.0, 300.0));
        input.commands.clear();
        send(&mut input, &session, &finger(9030, TouchPhase::Moved, 340.0, 320.0));
        assert!(painted(&input), "{:?}", input.commands);
        assert_eq!(input.handed(), Handed::default());
        send(&mut input, &session, &finger(9030, TouchPhase::Ended, 340.0, 320.0));
    }

    /// The mouse alone (no tablet pen moving it) shows where it hovers, paints at full pressure
    /// with its moves waiting for the frame, and hovers again once let go; its right button and
    /// the wheel paint nothing.
    #[test]
    fn the_mouse_hovers_and_paints_with_its_moves_handed_on_once_a_frame() {
        fn no_pen(_: (f32, f32)) -> Option<crate::pen::MousePen> {
            None
        }
        let session = shown();
        session.set_touch_settings(procraft_core::TouchSettings::default());
        let mut input = Input { read_mouse_pen: no_pen, test_clock: Some(0.0), ..Input::default() };
        let device_id = DeviceId::dummy();
        let moved = |x: f64| WindowEvent::CursorMoved { device_id, position: PhysicalPosition::new(x, 200.0) };
        let button = |button, state| WindowEvent::MouseInput { device_id, state, button };
        send(&mut input, &session, &moved(200.0));
        assert_eq!(input.hovering(), Some((200.0, 200.0)));
        // the right button pressed and dragged: nothing
        send(&mut input, &session, &button(MouseButton::Right, ElementState::Pressed));
        send(&mut input, &session, &moved(220.0));
        send(&mut input, &session, &button(MouseButton::Right, ElementState::Released));
        assert!(!painted(&input) && input.idle(), "{:?}", input.commands);
        // pressed, it paints at once, at the mouse's full pressure
        send(&mut input, &session, &button(MouseButton::Left, ElementState::Pressed));
        assert!(painted(&input) && input.pressing_from_afar() && input.hovering().is_none());
        input.commands.clear();
        for (k, x) in [230.0, 260.0, 290.0, 320.0].into_iter().enumerate() {
            input.test_clock = Some(8.0 * (k + 1) as f64);
            send(&mut input, &session, &moved(x));
        }
        assert_eq!(input.handed(), Handed::default(), "waiting for the frame");
        assert!(input.flush(&session));
        assert_eq!(input.handed(), Handed { batches: 1, samples: 4 });
        // let go: the stroke ends after the moves before it, and the mouse hovers again
        send(&mut input, &session, &moved(330.0));
        send(&mut input, &session, &button(MouseButton::Left, ElementState::Released));
        assert_eq!(input.handed(), Handed { batches: 2, samples: 5 });
        assert!(input.commands.contains(&TouchCommand::StrokeEnded), "{:?}", input.commands);
        assert_eq!(input.hovering(), Some((330.0, 200.0)));
    }

    /// A pen drawing a line for a second while no frame comes (a slow one): its moves wait, and
    /// no QuickShape wait runs out meanwhile (draw and hold waits only for a stroke seen going 15
    /// points), so the pen still moving snaps no shape; its moves handed on by the next frame, held
    /// still past the wait it snaps, as ever.
    #[test]
    fn a_pen_still_moving_snaps_no_shape_however_long_its_moves_wait() {
        let _one = crate::pen::tests::mouse_pointer_tests();
        let Some(window) = Window::open(1024) else { return };
        let session = &window.session;
        session.set_touch_settings(procraft_core::TouchSettings::default());
        let mut input = Input { read_mouse_pen: tablet_pen, test_clock: Some(0.0), ..Input::default() };
        TABLET_PEN.with(|p| p.set(Some(crate::pen::MousePen { pressure: 0.5, eraser: false })));
        let device_id = DeviceId::dummy();
        let moved = |x: f64| WindowEvent::CursorMoved { device_id, position: PhysicalPosition::new(x, 500.0) };
        send(&mut input, session, &moved(400.0));
        send(
            &mut input,
            session,
            &WindowEvent::MouseInput { device_id, state: ElementState::Pressed, button: MouseButton::Left },
        );
        for k in 1..=1000 {
            input.test_clock = Some(k as f64);
            send(&mut input, session, &moved(400.0 + k as f64 * 0.5));
        }
        assert_eq!(input.handed(), Handed::default(), "no frame came");
        input.tick(session);
        assert!(!input.commands.contains(&TouchCommand::ShapeSnapped), "{:?}", input.commands);
        // the next frame
        input.flush(session);
        assert_eq!(input.handed(), Handed { batches: 1, samples: 1000 });
        input.tick(session);
        assert!(!input.commands.contains(&TouchCommand::ShapeSnapped), "{:?}", input.commands);
        input.test_clock = Some(1000.0 + 700.0);
        input.tick(session);
        assert!(input.commands.contains(&TouchCommand::ShapeSnapped), "{:?}", input.commands);
        TABLET_PEN.with(|p| p.set(None));
    }

    /// A brush file (`BRUSH_FILE`) drawn through the desktop's input by a tablet's pen moving the
    /// mouse pointer, read in whole pixels (WinTab giving nothing finer), on a 2048 px canvas fitted
    /// to a 1920 by 1080 window: a circle, a triangle, a square and hatching, at 150 px a second,
    /// the pointer moved as the pen reaches each next pixel. Each is drawn twice: above as a stroke
    /// takes input it isn't told comes in whole pixels (its first sample half a pixel off them, the
    /// rest whole), below as it is. The artwork goes to `PROCRAFT_DUMP/whole-pixels.png`.
    /// `BRUSH_FILE=x.brush PROCRAFT_DUMP=dir cargo test -p procraft-desktop dump_whole_pixel_strokes -- --ignored`
    #[test]
    #[ignore]
    fn dump_whole_pixel_strokes() {
        let (Ok(file), Ok(dir)) = (std::env::var("BRUSH_FILE"), std::env::var("PROCRAFT_DUMP")) else { return };
        let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
        let session = CanvasSession::new(2048, 2048);
        session.set_display_scale(1.0);
        session.use_gpu(gpu);
        session.show_in(1920, 1080);
        let name = std::path::Path::new(&file).file_name().map(|n| n.to_string_lossy().into_owned());
        let set = session.import_any_brushes(std::fs::read(&file).unwrap(), name, 0).unwrap();
        assert!(session.select_brush(set, 0));
        session.set_color(0.0, 0.0, 0.0);
        TABLET_PEN.with(|p| p.set(Some(crate::pen::MousePen { pressure: 0.6, eraser: false })));
        let mut input = Input { read_mouse_pen: tablet_pen, ..Input::default() };
        let m = session.view_matrix();
        eprintln!("zoom {}", m[0].hypot(m[1]));
        // canvas pixel (x, y) on the window
        let shown = |(x, y): (f32, f32)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
        let device_id = DeviceId::dummy();
        let moved = |(x, y): (f32, f32)| WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(f64::from(x), f64::from(y)),
        };
        let press = |state| WindowEvent::MouseInput { device_id, state, button: MouseButton::Left };
        // a stroke through `corners` (canvas pixels), or round a circle
        let mut draw = |path: &dyn Fn(f32) -> (f32, f32), told: bool| {
            let length: f32 = (1..=400)
                .map(|i| {
                    let (a, b) = (shown(path((i - 1) as f32 / 400.0)), shown(path(i as f32 / 400.0)));
                    (a.0 - b.0).hypot(a.1 - b.1)
                })
                .sum();
            let (x, y) = shown(path(0.0));
            let mut last = (x.round(), y.round());
            send(&mut input, &session, &moved(if told { last } else { (last.0 + 0.5, last.1) }));
            send(&mut input, &session, &press(ElementState::Pressed));
            let begun = Instant::now();
            let steps = (length * 4.0) as usize;
            for i in 1..=steps {
                let (x, y) = shown(path(i as f32 / steps as f32));
                let at = (x.round(), y.round());
                if at != last {
                    // when the pen gets there
                    let due = Duration::from_secs_f32(i as f32 / 4.0 / 150.0);
                    while begun.elapsed() < due {
                        std::thread::sleep(Duration::from_micros(200));
                    }
                    send(&mut input, &session, &moved(at));
                    last = at;
                }
            }
            send(&mut input, &session, &press(ElementState::Released));
        };
        let along = |corners: Vec<(f32, f32)>| {
            move |t: f32| {
                let pieces = (corners.len() - 1) as f32;
                let at = (t * pieces).min(pieces - 1e-4);
                let (i, f) = (at.floor() as usize, at.fract());
                let (a, b) = (corners[i], corners[i + 1]);
                (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)
            }
        };
        for (row, told) in [(560.0f32, false), (1360.0, true)] {
            draw(
                &|t: f32| {
                    let a = std::f32::consts::TAU * t;
                    (560.0 + 130.0 * a.cos(), row + 120.0 * a.sin())
                },
                told,
            );
            draw(
                &along(vec![(820.0, row + 120.0), (950.0, row - 130.0), (1080.0, row + 120.0), (820.0, row + 120.0)]),
                told,
            );
            draw(
                &along(vec![
                    (1160.0, row - 110.0),
                    (1380.0, row - 115.0),
                    (1385.0, row + 110.0),
                    (1165.0, row + 115.0),
                    (1160.0, row - 110.0),
                ]),
                told,
            );
            for k in 0..6 {
                let x = 1480.0 + 30.0 * k as f32;
                draw(&along(vec![(x, row + 80.0), (x + 120.0, row - 80.0)]), told);
            }
        }
        session.export_png(format!("{dir}/whole-pixels.png")).unwrap();
    }
}
