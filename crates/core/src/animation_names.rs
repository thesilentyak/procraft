//! Animations by name (Aseprite's tags): a run of frames named "idle" or "walk", so one artwork
//! holds a character's moves. Each starts at a frame and runs to the next one's start, or to the
//! last frame that plays; playing loops the animation holding the current frame.

use procraft_engine::animation;
use procraft_engine::document::LayerId;

use crate::CanvasSession;

/// A named animation: its name and its frames, as `animation_frames` numbers them.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AnimationName {
    pub name: String,
    /// Its first frame, and how many it has.
    pub first: u32,
    pub count: u32,
}

#[uniffi::export]
impl CanvasSession {
    /// The named animations, in order.
    pub fn animation_names(&self) -> Vec<AnimationName> {
        let doc = &self.shared.lock().doc;
        animation::animations(doc)
            .into_iter()
            .filter_map(|(name, r)| {
                Some(AnimationName {
                    name: name?,
                    first: u32::try_from(r.start).ok()?,
                    count: u32::try_from(r.len()).ok()?,
                })
            })
            .collect()
    }

    /// The name of the animation holding frame `frame` (a layer's id), if it has one.
    pub fn animation_name_of(&self, frame: u64) -> Option<String> {
        let doc = &self.shared.lock().doc;
        let index = animation::frames(doc).iter().position(|f| f.0 == frame)?;
        animation::animation_of(doc, index)?.0
    }

    /// Whether an animation starts at frame `frame`.
    pub fn animation_starts_at(&self, frame: u64) -> bool {
        let doc = &self.shared.lock().doc;
        doc.animation.as_ref().is_some_and(|a| a.tags.iter().any(|t| t.from.0 == frame))
    }

    /// Starts an animation named `name` at frame `frame` (renaming one that starts there), one
    /// undo step. False with Animation Assist off or no such frame, or an empty name.
    pub fn start_animation(&self, frame: u64, name: String) -> bool {
        if name.trim().is_empty() {
            return false;
        }
        self.change("name animation", |d| {
            if !animation::frames(d).iter().any(|f| f.0 == frame) {
                return None;
            }
            d.animation.as_mut()?.set_tag(LayerId(frame), &name);
            Some(())
        })
        .is_some()
    }

    /// Renames the animation holding frame `frame`.
    pub fn rename_animation(&self, frame: u64, name: String) -> bool {
        match self.animation_start_of(frame) {
            Some(from) => self.start_animation(from, name),
            None => false,
        }
    }

    /// Removes the name of the animation holding frame `frame`: its frames join the one before.
    pub fn remove_animation(&self, frame: u64) -> bool {
        let Some(from) = self.animation_start_of(frame) else { return false };
        self.change("remove animation", |d| {
            d.animation.as_mut()?.remove_tag(LayerId(from));
            Some(())
        })
        .is_some()
    }

    /// The timeline's places that play while playing: those of `animation_played_places` whose
    /// frame is in the animation holding the current frame (every one without names).
    pub fn animation_playing_places(&self) -> Vec<u32> {
        let range = {
            let doc = &self.shared.lock().doc;
            animation::current_frame(doc).and_then(|i| animation::animation_of(doc, i)).map(|(_, r)| r)
        };
        let places = self.animation_played_places();
        let Some(range) = range else { return places };
        let sequence = self.animation_sequence();
        let mine: Vec<u32> = places
            .iter()
            .copied()
            .filter(|&p| sequence.get(p as usize).is_some_and(|&f| range.contains(&(f as usize))))
            .collect();
        if mine.is_empty() { places } else { mine }
    }
}

impl CanvasSession {
    /// The first frame (a layer's id) of the named animation holding frame `frame`.
    fn animation_start_of(&self, frame: u64) -> Option<u64> {
        let doc = &self.shared.lock().doc;
        let all = animation::frames(doc);
        let index = all.iter().position(|f| f.0 == frame)?;
        let (name, range) = animation::animation_of(doc, index)?;
        name?;
        all.get(range.start).map(|f| f.0)
    }
}
