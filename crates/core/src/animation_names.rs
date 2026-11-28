//! Animations by name (Aseprite's tags): a run of frames named "idle" or "walk", so one artwork
//! holds a character's moves. Each starts at a frame and runs to the next one's start, or to the
//! last frame that plays; playing loops the animation holding the current frame.

use procraft_engine::animation;
use procraft_engine::document::LayerId;
use procraft_engine::geom::IRect;

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

impl CanvasSession {
    /// Adds `frames` new frames holding `pixels` (each `area`'s size, stacked down, straight-alpha
    /// RGBA) as one undo step called `step`: Animation Assist turned on if it's off, the layers
    /// named "`layer_name` 1", "`layer_name` 2"… after the animation holding the current frame, and
    /// named an animation `tag` of their own, the first new frame current. None (nothing changed)
    /// without room for the layers or with `area` off the canvas; else the first new frame's id.
    pub(crate) fn add_animation_frames(
        &self,
        step: &str,
        area: IRect,
        pixels: &[u8],
        frames: u32,
        layer_name: &str,
        tag: &str,
    ) -> Option<u64> {
        let count = usize::try_from(frames).ok()?;
        let size = (area.width() as usize).checked_mul(area.height() as usize)?.checked_mul(4)?;
        if count == 0 || pixels.len() != size.checked_mul(count)? {
            return None;
        }
        let ids = self.change(step, |d| {
            if !d.has_room(count) || area.intersect(IRect::from_size(d.width(), d.height())) != area {
                return None;
            }
            if d.animation.is_none() {
                d.animation = Some(animation::Animation::starting(false, d.pixel_art));
            }
            // after the animation holding the current frame (else the current frame), out of any
            // group: an animation already there stays whole
            let all = animation::frames(d);
            let under = animation::current_frame(d)
                .map(|i| animation::animation_of(d, i).map_or(i, |(_, r)| r.end.saturating_sub(1).max(i)))
                .and_then(|i| all.get(i).copied())
                .or_else(|| d.members(None).last().copied());
            if let Some(under) = under {
                d.set_current(under);
            }
            let mut ids = Vec::with_capacity(count);
            for k in 1..=count {
                let id = d.add_layer();
                d.rename(id, &format!("{layer_name} {k}"));
                ids.push(id);
            }
            if let Some(&first) = ids.first() {
                d.set_current(first);
                if let Some(a) = d.animation.as_mut() {
                    a.set_tag(first, tag);
                }
            }
            Some(ids)
        })?;
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        if let (Some(canvas), Some(gpu)) = (st.canvas.as_mut(), st.gpu.as_ref()) {
            for (k, id) in ids.iter().enumerate() {
                let part = pixels.get(k * size..(k + 1) * size)?;
                canvas.upload_layer_part(gpu, *id, area, part).ok()?;
            }
            canvas.set_play_frame(None);
            canvas.invalidate();
        }
        st.revision += 1;
        self.shared.frame(st);
        ids.first().map(|id| id.0)
    }

    /// Inserts `frames` new frames holding `pixels` (each `area`'s size, stacked down,
    /// premultiplied RGBA) right after the top-level frame holding `after` (or the current frame
    /// if `after` is gone), staying inside the same named animation, as one undo step called
    /// `step`. Animation Assist turns on if it's off; the first new frame becomes current. None
    /// (nothing changed) without room for the layers or with `area` off the canvas; else the first
    /// new frame's id.
    #[cfg(feature = "generation")]
    pub(crate) fn insert_inbetween_frames(
        &self,
        step: &str,
        area: IRect,
        pixels: &[u8],
        frames: u32,
        after: u64,
        layer_name: &str,
    ) -> Option<u64> {
        let count = usize::try_from(frames).ok()?;
        let size = (area.width() as usize).checked_mul(area.height() as usize)?.checked_mul(4)?;
        if count == 0 || pixels.len() != size.checked_mul(count)? {
            return None;
        }
        let ids = self.change(step, |d| {
            if !d.has_room(count) || area.intersect(IRect::from_size(d.width(), d.height())) != area {
                return None;
            }
            if d.animation.is_none() {
                d.animation = Some(animation::Animation::starting(false, d.pixel_art));
            }
            let under = top_level_frame(d, LayerId(after))
                .or_else(|| {
                    let all = animation::frames(d);
                    animation::current_frame(d).and_then(|i| all.get(i).copied())
                })
                .or_else(|| d.members(None).last().copied());
            if let Some(under) = under {
                d.set_current(under);
            }
            let mut ids = Vec::with_capacity(count);
            for k in 1..=count {
                let id = d.add_layer();
                d.rename(id, &format!("{layer_name} {k}"));
                ids.push(id);
            }
            if let Some(&first) = ids.first() {
                d.set_current(first);
            }
            Some(ids)
        })?;
        let mut guard = self.shared.lock();
        let st = &mut *guard;
        if let (Some(canvas), Some(gpu)) = (st.canvas.as_mut(), st.gpu.as_ref()) {
            for (k, id) in ids.iter().enumerate() {
                let part = pixels.get(k * size..(k + 1) * size)?;
                canvas.upload_layer_part(gpu, *id, area, part).ok()?;
            }
            canvas.set_play_frame(None);
            canvas.invalidate();
        }
        st.revision += 1;
        self.shared.frame(st);
        ids.first().map(|id| id.0)
    }
}

/// The top-level layer or group holding `id`, if `id` is still in `doc`.
#[cfg(feature = "generation")]
fn top_level_frame(doc: &procraft_engine::document::Document, mut id: LayerId) -> Option<LayerId> {
    doc.layer(id)?;
    for _ in 0..64 {
        match doc.group_of(id) {
            Some(g) => id = g,
            None => return Some(id),
        }
    }
    Some(id)
}
