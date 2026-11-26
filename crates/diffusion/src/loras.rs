//! What's kept about each LoRA on the server: its trigger words and the strength it's given when
//! added to a style. One TOML file for every LoRA, in ComfyUI's own user folder
//! (`user/default/procraft-diffusion/loras.toml`), read and written through its `userdata` API,
//! so every device using the server shares it:
//!
//! ```toml
//! ["anima/dark_art_style_(zura_janai).safetensors"]
//! trigger = "dark_art_style"
//! strength = 0.8
//! ```

use serde_json::Value;
use toml_edit::{DocumentMut, Item, Table, value};

/// The file's path under ComfyUI's user folder, as `userdata` takes it.
pub const NOTES_PATH: &str = "procraft-diffusion/loras.toml";

/// What's kept about one LoRA.
#[derive(Clone, Debug, PartialEq)]
pub struct LoraNote {
    /// Its trigger words, comma-separated; empty: none.
    pub trigger: String,
    /// The strength it starts at in a style; none: 1.
    pub strength: Option<f32>,
}

/// The server's notes on its LoRAs, by file as the server lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoraNotes {
    notes: Vec<(String, LoraNote)>,
    /// Why the file couldn't be read, if it couldn't: it's then left alone rather than written over.
    pub problem: Option<String>,
}

impl LoraNotes {
    /// The notes in `text`; none, with the problem, if it can't be read. Entries that aren't
    /// tables are passed over.
    pub fn read(text: &str) -> LoraNotes {
        let doc: DocumentMut = match text.parse() {
            Ok(doc) => doc,
            Err(e) => return LoraNotes { notes: Vec::new(), problem: Some(crate::styles::plain_error(text, &e)) },
        };
        let notes = doc
            .iter()
            .filter_map(|(file, item)| {
                let table = item.as_table_like()?;
                let trigger = table.get("trigger").and_then(Item::as_str).unwrap_or_default().to_owned();
                let strength = table
                    .get("strength")
                    .and_then(|s| s.as_float().or_else(|| s.as_integer().map(|i| i as f64)))
                    .map(|s| s as f32)
                    .filter(|s| s.is_finite());
                Some((file.to_owned(), LoraNote { trigger, strength }))
            })
            .collect();
        LoraNotes { notes, problem: None }
    }

    /// The note on `file`, matched as the server lists it, or by its name in another folder.
    pub fn get(&self, file: &str) -> Option<&LoraNote> {
        let name = |f: &str| f.rsplit(['/', '\\']).next().unwrap_or(f).to_lowercase();
        self.notes
            .iter()
            .find(|(f, _)| f == file)
            .or_else(|| self.notes.iter().find(|(f, _)| name(f) == name(file)))
            .map(|(_, n)| n)
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}

/// The heading of a new notes file.
const HEADING: &str = "# Trigger words and default strengths for LoRAs, kept by ProCraft Diffusion.\n\
# One entry per LoRA file, as ComfyUI names it. Edit freely.\n\n";

/// The notes file's text with each `(file, trigger words)` of `changes` set: `old` (the file as it
/// is) edited in place, so other entries and comments stay; a new file when there's none. An entry
/// whose words are emptied keeps its other keys (its strength) and loses `trigger`. None if `old`
/// can't be read: a file someone wrote by hand isn't written over.
pub fn write_notes(old: Option<&str>, changes: &[(String, String)]) -> Option<String> {
    let mut doc: DocumentMut = match old {
        Some(text) if !text.trim().is_empty() => text.parse().ok()?,
        _ => {
            let mut doc = DocumentMut::new();
            doc.decor_mut().set_prefix(HEADING);
            doc
        }
    };
    for (file, trigger) in changes {
        let trigger = trigger.trim();
        if !doc.contains_key(file) {
            if trigger.is_empty() {
                continue;
            }
            doc.insert(file, Item::Table(Table::new()));
        }
        let Some(table) = doc.get_mut(file).and_then(Item::as_table_mut) else { continue };
        if trigger.is_empty() {
            table.remove("trigger");
        } else {
            table.insert("trigger", value(trigger));
        }
    }
    Some(doc.to_string())
}

/// How often a tag must be on the training pictures to be taken for a trigger word.
const TRIGGER_SHARE: f64 = 0.95;
/// How many more of the most used tags are offered beside them.
const MORE: usize = 8;

/// What a LoRA's own training notes suggest for its trigger words.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoraLook {
    /// The tags on nearly every training picture (95 %) that aren't ordinary Danbooru tags: its
    /// trigger words, most used first.
    pub filled: Vec<String>,
    /// The next most used tags, any kind, to add by hand.
    pub more: Vec<String>,
}

/// The trigger words LoRA metadata (`metadata`, as the server's `view_metadata` gives it)
/// suggests, from kohya's tag counts (`ss_tag_frequency`) and number of pictures
/// (`ss_dataset_dirs`). `dark_art_style` on 99 of 99 pictures is one; `1girl` on every picture
/// of a character isn't, being an ordinary tag; a known character's name is. Nothing when the
/// file has no such notes. NSFW tags are only among the others when `nsfw` (they're offered as
/// suggestions, as autocomplete's are).
pub fn look(metadata: &Value, nsfw: bool) -> LoraLook {
    let notes = |key: &str| -> Option<Value> {
        let value = metadata.get(key)?;
        match value.as_str() {
            Some(text) => serde_json::from_str(text).ok(),
            None => Some(value.clone()),
        }
    };
    // the tags of every folder trained on, counted together
    let mut counts: Vec<(String, u64)> = Vec::new();
    let mut most_in_a_folder = 0;
    if let Some(Value::Object(folders)) = notes("ss_tag_frequency") {
        for tags in folders.values().filter_map(Value::as_object) {
            let mut most = 0;
            for (tag, n) in tags {
                let (tag, n) = (tag.trim(), n.as_u64().unwrap_or(0));
                if tag.is_empty() || n == 0 {
                    continue;
                }
                most = most.max(n);
                match counts.iter_mut().find(|(t, _)| t == tag) {
                    Some((_, c)) => *c += n,
                    None => counts.push((tag.to_owned(), n)),
                }
            }
            most_in_a_folder += most;
        }
    }
    if counts.is_empty() {
        return LoraLook::default();
    }
    // the pictures, as the folders count them, else as many as the most used tag is on
    let pictures = match notes("ss_dataset_dirs") {
        Some(Value::Object(folders)) => {
            folders.values().filter_map(|f| f.get("img_count").and_then(Value::as_u64)).sum::<u64>()
        }
        _ => 0,
    };
    let pictures = if pictures == 0 { most_in_a_folder } else { pictures };
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let (filled, rest): (Vec<_>, Vec<_>) = counts
        .into_iter()
        .partition(|(tag, n)| *n as f64 >= pictures as f64 * TRIGGER_SHARE && !crate::tags::is_general(tag));
    LoraLook {
        filled: filled.into_iter().map(|(t, _)| t).collect(),
        more: rest.into_iter().filter(|(t, _)| nsfw || !crate::tags::is_nsfw(t)).take(MORE).map(|(t, _)| t).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_are_read_by_file() {
        let notes = LoraNotes::read(
            "# kept by ProCraft Diffusion\n\n[\"anima/dark_art.safetensors\"]\ntrigger = \"dark_art_style\"\nstrength = 0.8\n\n\
             [\"anima/dragon.safetensors\"]\ntrigger = \"@dragonball\"\nstrength = 1\n\nstray = 3\n",
        );
        assert_eq!(notes.problem, None);
        let dark = notes.get("anima/dark_art.safetensors").unwrap();
        assert_eq!((dark.trigger.as_str(), dark.strength), ("dark_art_style", Some(0.8)));
        assert_eq!(notes.get("moved/dragon.safetensors").unwrap().strength, Some(1.0));
        assert!(notes.get("stray").is_none());
    }

    #[test]
    fn notes_are_written_in_place() {
        let old = "# mine\n[\"a.safetensors\"]\ntrigger = \"old\"\nstrength = 0.8 # keep\n\n[\"b.safetensors\"]\ntrigger = \"bee\"\n";
        let changes = [
            ("a.safetensors".to_owned(), "new, words".to_owned()),
            ("b.safetensors".to_owned(), String::new()),
            ("anima/c.safetensors".to_owned(), "sea".to_owned()),
        ];
        let text = write_notes(Some(old), &changes).unwrap();
        assert!(text.starts_with("# mine"), "{text}");
        assert!(text.contains("strength = 0.8 # keep"), "{text}");
        let notes = LoraNotes::read(&text);
        assert_eq!(notes.get("a.safetensors").unwrap().trigger, "new, words");
        assert_eq!(notes.get("b.safetensors").unwrap().trigger, "");
        assert_eq!(notes.get("anima/c.safetensors").unwrap().trigger, "sea");
        let fresh = write_notes(None, &changes[2..]).unwrap();
        assert!(fresh.starts_with("# Trigger words"), "{fresh}");
        assert!(write_notes(Some("[\"a\"\n"), &changes).is_none(), "a broken file is left alone");
    }

    #[test]
    fn trigger_words_come_from_the_training_notes() {
        // made up: a character LoRA, `1girl` on every picture, its trigger word and the character
        let frequency = serde_json::json!({
            "10_pics": {"zz_teal_style": 40, "1girl": 40, "hatsune miku": 39, "solo": 30, "long hair": 20},
            "2_more": {"zz_teal_style": 10, "1girl": 10, "hatsune miku": 10, "smile": 4}
        });
        let metadata = serde_json::json!({
            "ss_tag_frequency": frequency.to_string(),
            "ss_dataset_dirs": serde_json::json!({"10_pics": {"n_repeats": 10, "img_count": 40},
                                                   "2_more": {"n_repeats": 2, "img_count": 10}}).to_string(),
        });
        let found = look(&metadata, true);
        assert_eq!(found.filled, ["zz_teal_style", "hatsune miku"]);
        assert_eq!(found.more, ["1girl", "solo", "long hair", "smile"]);
        // without the folders' picture counts, the most used tag stands for them
        let found = look(&serde_json::json!({ "ss_tag_frequency": frequency.to_string() }), true);
        assert_eq!(found.filled, ["zz_teal_style", "hatsune miku"]);
        assert_eq!(look(&Value::Null, false), LoraLook::default());
    }

    #[test]
    fn a_file_that_cant_be_read_is_noted() {
        let notes = LoraNotes::read("[\"a\"\ntrigger = ");
        assert!(notes.is_empty() && notes.problem.is_some());
    }
}
