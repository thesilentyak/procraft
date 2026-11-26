//! A stroke's pressure as the plugins on the pressure hook give it (see
//! [`crate::hooks::Hooks::pressure`]): each sample asked once, in the order they come, before
//! anything else takes it; predicted input never asked, taking the last pressure given.

use procraft_engine::stroke::StrokeBuilder;

use crate::plugin_run::PressureAsked;
use crate::{PenSample, PointerKind, State};

/// The open stroke's pressure as the plugins give it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Pressing {
    /// The stroke's pointer, numbered as the hook numbers it, and when its first sample came
    /// (milliseconds, the input's clock).
    pointer: i32,
    first_ms: f64,
    /// A plugin gave its first sample a pressure: the stroke draws with the pressure given though
    /// its pointer reports none (a finger).
    pub(crate) given: bool,
    /// The last pressure given, which predicted input takes.
    last: Option<f32>,
    /// How far its pressure grows from nothing at its start and falls to nothing at its end,
    /// window points.
    pub(crate) tapers: [f32; 2],
}

impl Pressing {
    /// `builder` drawing with this pressure, `per_point` canvas pixels (as its samples come) a
    /// window point.
    pub(crate) fn applied(&self, builder: StrokeBuilder, per_point: f32) -> StrokeBuilder {
        let builder = if self.given { builder.with_given_pressure() } else { builder };
        if self.tapers == [0.0; 2] || !per_point.is_finite() {
            return builder;
        }
        builder.with_pressure_tapers(self.tapers[0] * per_point, self.tapers[1] * per_point)
    }
}

/// `builder` pressed as `like` was (the stroke it lays again).
pub(crate) fn pressed_like(builder: StrokeBuilder, like: &StrokeBuilder) -> StrokeBuilder {
    let (given, [start, end]) = like.pressing();
    let builder = if given { builder.with_given_pressure() } else { builder };
    builder.with_pressure_tapers(start, end)
}

/// How the pressure hook numbers a pointer.
pub(crate) fn pointer_number(pointer: PointerKind) -> i32 {
    match pointer {
        PointerKind::Finger => 0,
        PointerKind::Stylus => 1,
        PointerKind::StylusEraser => 2,
        PointerKind::Mouse => 3,
    }
}

impl State {
    /// `samples` as the plugins on the pressure hook press them: a stroke's first by `pointer`, or
    /// more of the open one (`pointer` none). Nothing changes with none on it.
    pub(crate) fn press(&mut self, pointer: Option<PointerKind>, samples: &mut [PenSample]) {
        // a stroke starts with nothing given, whatever the last was given
        if let Some(pointer) = pointer {
            let first_ms = samples.first().map_or(0.0, |s| s.time_ms);
            self.pressing = Pressing { pointer: pointer_number(pointer), first_ms, ..Pressing::default() };
        }
        let Some(hooks) = self.hooks.clone().filter(|h| h.listens(crate::plugin_run::Hook::Pressure)) else { return };
        let per_point = if self.display_scale > 0.0 { 1.0 / self.display_scale } else { 1.0 };
        let p = &mut self.pressing;
        for (i, s) in samples.iter_mut().enumerate() {
            let first = pointer.is_some() && i == 0;
            let ms = (s.time_ms - p.first_ms) as f32;
            let asked = PressureAsked {
                pointer: p.pointer,
                first,
                x: s.x * per_point,
                y: s.y * per_point,
                ms,
                pressure: s.pressure,
            };
            let said = hooks.pressure(asked);
            if first {
                p.given = said.is_some() && !matches!(pointer, Some(PointerKind::Stylus | PointerKind::StylusEraser));
            }
            // a sample the plugins leave, in a stroke drawn with the pressure given, takes the last
            if let Some(pressure) = said.or(p.last.filter(|_| p.given)) {
                s.pressure = pressure;
                p.last = Some(pressure);
            }
        }
        if pointer.is_some() {
            p.tapers = hooks.pressure_tapers(p.pointer);
        }
    }

    /// Predicted input for the open stroke, pressed as its last sample was.
    pub(crate) fn press_predicted(&self, samples: &mut [PenSample]) {
        if let Some(last) = self.pressing.last {
            for s in samples {
                s.pressure = last;
            }
        }
    }

    /// Canvas pixels a window point, as the stroke's samples come onto the canvas.
    pub(crate) fn canvas_per_point(&self) -> f32 {
        self.view.map_or(1.0, |v| self.display_scale / v.zoom().max(1e-6))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointers_are_numbered_as_the_hook_says() {
        let numbers = [PointerKind::Finger, PointerKind::Stylus, PointerKind::StylusEraser, PointerKind::Mouse]
            .map(pointer_number);
        assert_eq!(numbers, [0, 1, 2, 3]);
    }

    #[test]
    fn with_nothing_given_a_builder_is_left_as_it_was() {
        use procraft_engine::{brush::Brush, input::Pointer};
        let plain = || StrokeBuilder::new(Brush::round_pen(), 10.0, Pointer::Finger);
        assert_eq!(Pressing::default().applied(plain(), 1.0).pressing(), (false, [0.0; 2]));
        let given = Pressing { given: true, tapers: [10.0, 20.0], ..Pressing::default() };
        assert_eq!(given.applied(plain(), 0.5).pressing(), (true, [5.0, 10.0]));
        let like = given.applied(plain(), 2.0);
        assert_eq!(pressed_like(plain(), &like).pressing(), (true, [20.0, 40.0]));
    }
}
