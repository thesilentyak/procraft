//! Named layouts, Edit layout's: the places of the interface's bars and of the floating panels (each
//! for each way the screen turns, and which windows are open, folded, hidden or pinned) saved under a
//! name of the user's, chosen again to bring them back, renamed and deleted. Kept with the
//! interface's other settings under [`named_layouts_key`]; each app keeps its own.
//!
//! A layout saved under a name already kept takes its place. Names are the user's words, trimmed,
//! without the characters that would break a line, at most [`MOST_NAME`] characters; at most
//! [`MOST_LAYOUTS`] are kept.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::bars::BarLayout;
use crate::layout::PanelLayout;

/// What the layouts are kept under.
const KEY: &str = "named_layouts";
/// The most layouts kept, and the longest name (characters).
pub(crate) const MOST_LAYOUTS: usize = 16;
pub(crate) const MOST_NAME: usize = 40;

/// One layout: its name, and the bars' and the panels' places as each keeps them.
#[derive(Clone, Debug, PartialEq)]
struct Named {
    name: String,
    bars: String,
    panels: String,
}

#[derive(Debug, Default)]
struct Inner {
    layouts: Vec<Named>,
    /// The one last saved or chosen, while it is kept.
    chosen: Option<String>,
}

/// The named layouts; see the module's words.
#[derive(Debug, Default, uniffi::Object)]
pub struct NamedLayouts {
    inner: Mutex<Inner>,
}

/// What the named layouts are kept under, with the interface's other settings.
#[uniffi::export]
pub fn named_layouts_key() -> String {
    KEY.to_owned()
}

/// `name` as a layout's name: without what would break a line or a field, trimmed, at most
/// [`MOST_NAME`] characters; none when nothing is left.
fn name_of(name: &str) -> Option<String> {
    let clean: String = name.chars().filter(|c| !c.is_control()).collect();
    let trimmed: String = clean.trim().chars().take(MOST_NAME).collect();
    let trimmed = trimmed.trim_end().to_owned();
    (!trimmed.is_empty()).then_some(trimmed)
}

#[uniffi::export]
impl NamedLayouts {
    #[uniffi::constructor]
    pub fn new() -> Arc<NamedLayouts> {
        Arc::new(NamedLayouts::default())
    }

    /// The layouts as kept ([`NamedLayouts::kept`]), leniently: a line that doesn't read is passed
    /// over.
    pub fn read_kept(&self, kept: String) {
        let mut inner = self.lock();
        inner.layouts.clear();
        inner.chosen = None;
        for line in kept.lines() {
            // (a layout's line never starts with a tab: its name is never nothing)
            if let Some(chosen) = line.strip_prefix("\tchosen\t") {
                inner.chosen = name_of(chosen);
                continue;
            }
            let mut fields = line.split('\t');
            let (Some(name), Some(bars), Some(panels)) = (fields.next(), fields.next(), fields.next()) else {
                continue;
            };
            let Some(name) = name_of(name) else { continue };
            if inner.layouts.len() < MOST_LAYOUTS && !inner.layouts.iter().any(|l| l.name == name) {
                inner.layouts.push(Named { name, bars: bars.to_owned(), panels: panels.to_owned() });
            }
        }
        let known = inner.chosen.as_ref().is_some_and(|c| inner.layouts.iter().any(|l| &l.name == c));
        if !known {
            inner.chosen = None;
        }
    }

    /// The layouts as kept: a line for each (its name, the bars' places and the panels', a tab
    /// between), and which was last saved or chosen (a line of its own, starting with a tab).
    pub fn kept(&self) -> String {
        let inner = self.lock();
        let mut lines: Vec<String> =
            inner.layouts.iter().map(|l| format!("{}\t{}\t{}", l.name, l.bars, l.panels)).collect();
        if let Some(chosen) = &inner.chosen {
            lines.push(format!("\tchosen\t{chosen}"));
        }
        lines.join("\n")
    }

    /// The layouts' names, in the order they were first saved.
    pub fn names(&self) -> Vec<String> {
        self.lock().layouts.iter().map(|l| l.name.clone()).collect()
    }

    /// The layout last saved or chosen, while it is kept.
    pub fn chosen(&self) -> Option<String> {
        self.lock().chosen.clone()
    }

    /// A name for a layout saved now: "Layout" and the first number no layout's name has.
    pub fn next_name(&self) -> String {
        let inner = self.lock();
        (1..)
            .map(|n| format!("Layout {n}"))
            .find(|name| !inner.layouts.iter().any(|l| &l.name == name))
            .unwrap_or_else(|| "Layout".to_owned())
    }

    /// The bars' and the panels' places as they are now, saved under `name` (in place of one of
    /// that name): the name as kept, or none for a name of nothing, or a new one past the most.
    pub fn save(&self, name: String, bars: Arc<BarLayout>, panels: Arc<PanelLayout>) -> Option<String> {
        let name = name_of(&name)?;
        let named = Named { name: name.clone(), bars: bars.kept(), panels: panels.kept() };
        let mut inner = self.lock();
        let count = inner.layouts.len();
        match inner.layouts.iter().position(|l| l.name == name).and_then(|at| inner.layouts.get_mut(at)) {
            Some(old) => *old = named,
            None if count < MOST_LAYOUTS => inner.layouts.push(named),
            None => return None,
        }
        inner.chosen = Some(name.clone());
        Some(name)
    }

    /// The layout named `name` brought back: the bars and the panels where it had them, and only
    /// those (what it doesn't have goes back where it always is). False for a name not kept.
    pub fn choose(&self, name: String, bars: Arc<BarLayout>, panels: Arc<PanelLayout>) -> bool {
        let mut inner = self.lock();
        let Some(named) = inner.layouts.iter().find(|l| l.name == name).cloned() else { return false };
        bars.replace_kept(&named.bars);
        panels.replace_kept(&named.panels);
        inner.chosen = Some(named.name);
        true
    }

    /// The layout named `from` renamed `to`: the name as kept, or none for a name not kept, a new
    /// name of nothing or one another layout has.
    pub fn rename(&self, from: String, to: String) -> Option<String> {
        let to = name_of(&to)?;
        let mut inner = self.lock();
        if to != from && inner.layouts.iter().any(|l| l.name == to) {
            return None;
        }
        let named = inner.layouts.iter_mut().find(|l| l.name == from)?;
        named.name = to.clone();
        if inner.chosen.as_deref() == Some(from.as_str()) {
            inner.chosen = Some(to.clone());
        }
        Some(to)
    }

    /// The layout named `name` deleted: false for a name not kept.
    pub fn delete(&self, name: String) -> bool {
        let mut inner = self.lock();
        let before = inner.layouts.len();
        inner.layouts.retain(|l| l.name != name);
        if inner.chosen.as_deref() == Some(name.as_str()) {
            inner.chosen = None;
        }
        inner.layouts.len() < before
    }
}

impl NamedLayouts {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bars::{BarPlace, LayoutBar};
    use crate::floating::FloatingRoom;
    use crate::layout::LayoutPiece;

    /// The bars in a window 1600 by 1000, the top bar's groups 420 and 300 long at its top corners,
    /// the side bar on the left; editing.
    fn bars() -> Arc<BarLayout> {
        let bars = BarLayout::new();
        bars.set_window(1600.0, 1000.0, 16.0);
        bars.set_bar(LayoutBar::TopLeft, BarPlace { x: 0.0, y: 0.0, vertical: false }, 420.0, 48.0);
        bars.set_bar(LayoutBar::TopRight, BarPlace { x: 1300.0, y: 0.0, vertical: false }, 300.0, 48.0);
        bars.set_bar(LayoutBar::SideBar, BarPlace { x: 0.0, y: 240.0, vertical: true }, 519.0, 51.0);
        bars.set_editing(true);
        bars
    }

    /// The panels in the same window, every one allowed to float, laid out.
    fn panels() -> Arc<PanelLayout> {
        let panels = PanelLayout::new();
        panels.let_every_piece_float();
        panels.set_room(FloatingRoom { left: 0.0, top: 0.0, right: 1600.0, bottom: 1000.0 }, 1600.0, 1000.0, 48.0, 1.0);
        panels
    }

    fn moved(bars: &BarLayout, bar: LayoutBar, from: (f32, f32), to: (f32, f32)) -> BarPlace {
        assert!(bars.hold(bar, from.0, from.1));
        let place = bars.drag(to.0, to.1).unwrap();
        bars.let_go();
        place
    }

    #[test]
    fn a_layout_saved_comes_back_when_chosen_with_only_what_it_had() {
        let (bars, panels, named) = (bars(), panels(), NamedLayouts::new());
        // "Painting": the right half moved down to the bottom, the Layers panel floating on the right
        let half = moved(&bars, LayoutBar::TopRight, (1400.0, 10.0), (700.0, 955.0));
        panels.open_at(LayoutPiece::Layers, 800.0, 400.0, 0.0);
        panels.let_go();
        let layers = panels.reset_place(LayoutPiece::Layers).unwrap();
        assert_eq!(
            named.save("  Painting ".to_owned(), Arc::clone(&bars), Arc::clone(&panels)),
            Some("Painting".to_owned())
        );
        // then everything changed: the bars back, the Layers panel closed, the Actions panel out
        bars.reset();
        panels.close(LayoutPiece::Layers);
        panels.open_at(LayoutPiece::Actions, 800.0, 400.0, 0.0);
        panels.let_go();
        assert_eq!(named.next_name(), "Layout 1");
        assert_eq!(named.save(named.next_name(), Arc::clone(&bars), Arc::clone(&panels)), Some("Layout 1".to_owned()));
        assert_eq!(named.names(), ["Painting", "Layout 1"]);
        assert_eq!(named.next_name(), "Layout 2");
        // chosen: the half where it was saved, the Layers panel open where it was, and the Actions
        // panel, which it didn't have out, back where it always is
        assert!(named.choose("Painting".to_owned(), Arc::clone(&bars), Arc::clone(&panels)));
        assert_eq!(bars.shown(LayoutBar::TopRight), Some(half));
        assert_eq!(panels.shown(LayoutPiece::Layers), Some(layers));
        assert_eq!(panels.shown(LayoutPiece::Actions), None);
        assert_eq!(named.chosen().as_deref(), Some("Painting"));
        // and the other: the bars where they always are
        assert!(named.choose("Layout 1".to_owned(), Arc::clone(&bars), Arc::clone(&panels)));
        assert!(!bars.is_moved(LayoutBar::TopRight));
        assert!(panels.shown(LayoutPiece::Actions).is_some());
        assert_eq!(panels.shown(LayoutPiece::Layers), None);
        assert!(!named.choose("Nothing".to_owned(), bars, panels));
    }

    #[test]
    fn a_layout_keeps_each_way_the_screen_turns() {
        let (bars, panels, named) = (bars(), panels(), NamedLayouts::new());
        let landscape = moved(&bars, LayoutBar::TopLeft, (10.0, 10.0), (10.0 + 870.0, 10.0));
        // turned, moved again its own way
        bars.set_window(1000.0, 1600.0, 16.0);
        let portrait = moved(&bars, LayoutBar::TopLeft, (10.0, 10.0), (10.0 + 300.0, 10.0 + 700.0));
        named.save("Both".to_owned(), Arc::clone(&bars), Arc::clone(&panels));
        bars.reset();
        assert!(named.choose("Both".to_owned(), Arc::clone(&bars), Arc::clone(&panels)));
        assert_eq!(bars.shown(LayoutBar::TopLeft), Some(portrait));
        bars.set_window(1600.0, 1000.0, 16.0);
        assert_eq!(bars.shown(LayoutBar::TopLeft), Some(landscape));
    }

    #[test]
    fn layouts_are_renamed_deleted_and_kept_with_names_of_the_users_own() {
        let (bars, panels, named) = (bars(), panels(), NamedLayouts::new());
        let save = |name: &str| named.save(name.to_owned(), Arc::clone(&bars), Arc::clone(&panels));
        assert_eq!(save("Inking"), Some("Inking".to_owned()));
        assert_eq!(save("Sketch"), Some("Sketch".to_owned()));
        // a name of nothing, or what breaks a line, is no name
        assert_eq!(save("   "), None);
        assert_eq!(save("\t\n"), None);
        assert_eq!(save("Two\nlines"), Some("Twolines".to_owned()));
        // a long one is cut to 40 characters
        let long = "x".repeat(60);
        assert_eq!(save(&long).map(|n| n.chars().count()), Some(MOST_NAME));
        // saved again under a name kept: in its place, not a second
        assert_eq!(save("Inking"), Some("Inking".to_owned()));
        assert_eq!(named.names().iter().filter(|n| *n == "Inking").count(), 1);
        // renamed, not onto another's name
        assert_eq!(named.rename("Sketch".to_owned(), "Rough".to_owned()), Some("Rough".to_owned()));
        assert_eq!(named.rename("Rough".to_owned(), "Inking".to_owned()), None);
        assert_eq!(named.rename("Gone".to_owned(), "Anything".to_owned()), None);
        // the one chosen follows its name, and goes when it is deleted
        assert!(named.choose("Rough".to_owned(), Arc::clone(&bars), Arc::clone(&panels)));
        assert_eq!(named.rename("Rough".to_owned(), "Rough 2".to_owned()), Some("Rough 2".to_owned()));
        assert_eq!(named.chosen().as_deref(), Some("Rough 2"));
        // kept, and read back by the next start
        let next = NamedLayouts::new();
        next.read_kept(named.kept());
        assert_eq!(next.names(), named.names());
        assert_eq!(next.chosen(), named.chosen());
        assert!(named.delete("Rough 2".to_owned()));
        assert!(!named.delete("Rough 2".to_owned()));
        assert_eq!(named.chosen(), None);
        // at most 16
        for n in 0..30 {
            save(&format!("Many {n}"));
        }
        assert_eq!(named.names().len(), MOST_LAYOUTS);
        assert_eq!(save("One more"), None);
        // what doesn't read is passed over
        let lenient = NamedLayouts::new();
        lenient.read_kept("no tabs here\nGood\tbars\tpanels\n\t\t\nchosen\tNot kept".to_owned());
        assert_eq!(lenient.names(), ["Good"]);
        assert_eq!(lenient.chosen(), None);
    }
}
