//! Brush library files: a zip of the library's folders as they are kept, one per set (named by the
//! set) holding each of its brushes as a brush file (`<brush>.brush`) and an empty `.empty`; in
//! each set's folder and at the top, a list (the meta file, `.<extension>_meta.<32 hex>`).
//!
//! A list is a line per thing in its folder, `<name>/<order>/<modified>`, the lines sorted by
//! name and closed by a zero byte: a thing's place is its line's place plus its `order` (the sets
//! at the top, the brushes in a set). Then properties to the end, each a key (closed by a zero
//! byte), a time (u64, little end first), the value's length (the same) and the value: at the top
//! the set each tool's brush is in (`default_set_<tool>`), in a set the stock set whose icon it
//! shows (`icon_preset`) and each tool's brush there by name (`default_brush_<tool>`); tools 0
//! paint, 1 smudge, 2 erase.

use crate::brush::Brush;
use crate::brushfile::{self, BrushFile, Pictures};
use crate::zip::{Ready, ZipReader, ZipWriter};

/// The file extension of a brush library.
pub const EXTENSION: &str = "brushlibrary";

/// The tools a library keeps a brush for, in its numbering: paint, smudge, erase.
pub const TOOLS: usize = 3;

/// The empty file that keeps a set's folder.
const EMPTY: &str = ".empty";

/// The start of a list's name, the hex digits after it its own.
fn meta_prefix() -> String {
    format!(".{}_meta.", crate::native::EXTENSION)
}

/// A brush library as its file holds it: its sets in order, and each tool's brush (set, brush).
/// Its brushes as they are read ([`read_with`] can make them something else as it goes).
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryFile<B = BrushFile> {
    pub sets: Vec<LibrarySetFile<B>>,
    pub tools: [Option<(usize, usize)>; TOOLS],
}

/// A set in a library file: its name, the stock set whose icon it shows, and its brushes.
#[derive(Clone, Debug, PartialEq)]
pub struct LibrarySetFile<B = BrushFile> {
    pub name: String,
    pub icon: Option<String>,
    pub brushes: Vec<B>,
}

/// A list: its lines (name, order) in their order, and its properties (key, value).
#[derive(Clone, Debug, Default, PartialEq)]
struct Meta {
    lines: Vec<(String, i64)>,
    properties: Vec<(String, Vec<u8>)>,
}

impl Meta {
    fn read(bytes: &[u8]) -> Option<Meta> {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let text = std::str::from_utf8(bytes.get(..end)?).ok()?;
        let lines = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.rsplitn(3, '/');
                let (_modified, order, name) = (parts.next()?, parts.next()?, parts.next()?);
                Some((name.to_owned(), order.trim().parse().ok()?))
            })
            .collect();
        // properties until they run out (a damaged one ends them)
        let mut properties = Vec::new();
        let mut rest = bytes.get(end + 1..).unwrap_or_default();
        while let Some(zero) = rest.iter().position(|&b| b == 0) {
            let (Some(key), Some(length)) = (rest.get(..zero), rest.get(zero + 9..zero + 17)) else { break };
            let length = u64::from_le_bytes(length.try_into().ok()?) as usize;
            let Some(value) = rest.get(zero + 17..).and_then(|r| r.get(..length)) else { break };
            properties.push((String::from_utf8_lossy(key).into_owned(), value.to_vec()));
            rest = rest.get(zero + 17 + length..).unwrap_or_default();
        }
        Some(Meta { lines, properties })
    }

    fn property(&self, key: &str) -> Option<String> {
        let (_, v) = self.properties.iter().find(|(k, _)| k == key)?;
        Some(String::from_utf8_lossy(v).into_owned())
    }

    /// `names` in the list's order: each listed one at its place, the rest after them as given.
    fn order(&self, names: &[String]) -> Vec<usize> {
        let mut placed: Vec<(i64, usize)> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let line = self.lines.iter().enumerate().find(|(_, (n, _))| n == name);
                (line.map_or(i64::MAX, |(line, (_, order))| line as i64 + order), i)
            })
            .collect();
        placed.sort();
        placed.into_iter().map(|(_, i)| i).collect()
    }

    /// The list of `names` in that order, with `properties` stamped `time`.
    fn write(names: &[String], properties: &[(String, String)], time: u64) -> Vec<u8> {
        let mut sorted: Vec<(usize, &String)> = names.iter().enumerate().collect();
        sorted.sort_by(|a, b| a.1.as_bytes().cmp(b.1.as_bytes()));
        let mut out = Vec::new();
        for (line, (place, name)) in sorted.iter().enumerate() {
            out.extend(format!("{name}/{}/0\n", *place as i64 - line as i64).bytes());
        }
        out.push(0);
        for (key, value) in properties {
            out.extend(key.bytes());
            out.push(0);
            out.extend(time.to_le_bytes());
            out.extend((value.len() as u64).to_le_bytes());
            out.extend(value.bytes());
        }
        out
    }
}

/// Whether a zip holding `names` is a brush library: a list at its top, brush files in folders.
pub fn is_library<'n>(names: impl Iterator<Item = &'n str>) -> bool {
    let prefix = meta_prefix();
    let (mut listed, mut brushes) = (false, false);
    for n in names {
        listed |= n.starts_with(&prefix);
        brushes |= n.split_once('/').is_some_and(|(_, file)| !file.contains('/') && file.ends_with(".brush"));
        if listed && brushes {
            return true;
        }
    }
    false
}

/// Reads a brush library file (its empty sets too); none if it isn't one or holds no brushes.
pub fn read(bytes: &[u8]) -> Option<LibraryFile> {
    read_with(&ZipReader::new(bytes)?, Some)
}

/// Reads the brush library file that is the archive `zip` as [`read`] does, each brush made
/// `make` of it as soon as it is read (a few side by side, a set's brush files read one at a time
/// from the archive), so a big library is never held whole; a brush `make` makes nothing of is
/// left out.
pub fn read_with<B: Send>(zip: &ZipReader<'_>, make: impl Fn(BrushFile) -> Option<B> + Sync) -> Option<LibraryFile<B>> {
    let prefix = meta_prefix();
    let names: Vec<String> = zip.names().map(str::to_owned).collect();
    let meta_in = |folder: Option<&str>| {
        let name = names.iter().find(|n| match (folder, n.split_once('/')) {
            (None, None) => n.starts_with(&prefix),
            (Some(f), Some((d, file))) => d == f && file.starts_with(&prefix),
            _ => false,
        })?;
        Meta::read(&zip.read(name)?)
    };
    // (a brush file read and made at once: no more of them held at a time than there are threads)
    let made = |path: &String| zip.read(path).and_then(|b| brushfile::read(&b)).and_then(&make);
    // the sets' folders (an empty one kept by its empty file) and their brush files, as the zip
    // has them
    let mut folders: Vec<(String, Vec<String>)> = Vec::new();
    for n in &names {
        let Some((folder, file)) = n.split_once('/') else { continue };
        let brush = file.ends_with(".brush");
        if file.contains('/') || !(brush || file == EMPTY) {
            continue;
        }
        let file = brush.then(|| file.to_owned());
        match folders.iter_mut().find(|(f, _)| f == folder) {
            Some((_, files)) => files.extend(file),
            None => folders.push((folder.to_owned(), file.into_iter().collect())),
        }
    }
    let top = meta_in(None).unwrap_or_default();
    let set_names: Vec<String> = folders.iter().map(|(f, _)| f.clone()).collect();
    let mut sets = Vec::new();
    // each set's brushes by their file's name, for the tools
    let mut stems: Vec<Vec<String>> = Vec::new();
    let mut metas: Vec<Meta> = Vec::new();
    for s in top.order(&set_names) {
        let Some((folder, files)) = folders.get(s) else { continue };
        let meta = meta_in(Some(folder)).unwrap_or_default();
        let ordered: Vec<&String> = meta.order(files).into_iter().filter_map(|f| files.get(f)).collect();
        let paths: Vec<String> = ordered.iter().map(|file| format!("{folder}/{file}")).collect();
        let mut brushes = Vec::new();
        let mut kept = Vec::new();
        for (file, brush) in ordered.iter().zip(crate::par::side_by_side(&paths, made)) {
            let Some(brush) = brush.flatten() else { continue };
            brushes.push(brush);
            kept.push(file.trim_end_matches(".brush").to_owned());
        }
        sets.push(LibrarySetFile { name: folder.clone(), icon: meta.property("icon_preset"), brushes });
        stems.push(kept);
        metas.push(meta);
    }
    if sets.iter().all(|s| s.brushes.is_empty()) {
        return None;
    }
    let tools = std::array::from_fn(|tool| {
        let set = top.property(&format!("default_set_{tool}"))?;
        let s = sets.iter().position(|x| x.name == set)?;
        let brush = metas.get(s)?.property(&format!("default_brush_{tool}"))?;
        Some((s, stems.get(s)?.iter().position(|b| *b == brush)?))
    });
    Some(LibraryFile { sets, tools })
}

/// A set to write: its name, the stock set whose icon it shows, its brushes (each with its author
/// and pictures).
pub struct SetOut<'a> {
    pub name: &'a str,
    pub icon: Option<&'a str>,
    pub brushes: Vec<(&'a Brush, &'a str, Pictures<'a>)>,
}

/// `name` as a file's name: no folder marks or leading dots, never blank; numbered after it
/// ("Name 2") while `taken` has it.
fn file_name(name: &str, taken: &[String]) -> String {
    let clean: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '\0') { '-' } else { c }).collect();
    let clean = clean.trim().trim_start_matches('.').trim();
    let base = if clean.is_empty() { "Untitled" } else { clean };
    std::iter::once(base.to_owned())
        .chain((2..).map(|n| format!("{base} {n}")))
        .find(|n| !taken.iter().any(|t| t.eq_ignore_ascii_case(n)))
        .unwrap_or_default()
}

/// Writes a brush library file of `sets` in order, each tool's brush (paint, smudge, erase) at
/// `tools`; `id` gives each list's 32 hex digits (0 the top's, then the sets'), `time` stamps them
/// (seconds since 1970).
pub fn write(
    sets: &[SetOut<'_>],
    tools: [Option<(usize, usize)>; TOOLS],
    id: impl Fn(usize) -> String,
    time: u64,
) -> Vec<u8> {
    let heads: Vec<(&str, Option<&str>)> = sets.iter().map(|s| (s.name, s.icon)).collect();
    let mut library = LibraryWriter::begin(Vec::new(), &heads, tools, id, time);
    for (s, set) in sets.iter().enumerate() {
        for (brush, author, pictures) in &set.brushes {
            library.add(s, brush, author, pictures);
        }
    }
    library.finish().unwrap_or_default()
}

/// A brush library file written into a writer a brush at a time: each set's folder, its brushes'
/// files as they are added and its list once the next set begins, then the library's list, so
/// that however big the library, only the brush being added is held.
pub struct LibraryWriter<W: std::io::Write> {
    zip: ZipWriter<W>,
    /// Each set's folder and the stock set whose icon it shows.
    folders: Vec<String>,
    icons: Vec<Option<String>>,
    tools: [Option<(usize, usize)>; TOOLS],
    /// Each list's 32 hex digits (the top's, then the sets'), and when they are stamped.
    ids: Vec<String>,
    time: u64,
    /// The sets begun so far, and the brushes' file names in the last of them.
    begun: usize,
    files: Vec<String>,
}

impl<W: std::io::Write> LibraryWriter<W> {
    /// A library of the sets `heads` (each its name and icon) in order, each tool's brush (paint,
    /// smudge, erase) at `tools`, written into `out`; `id` and `time` as [`write`] takes them.
    pub fn begin(
        out: W,
        heads: &[(&str, Option<&str>)],
        tools: [Option<(usize, usize)>; TOOLS],
        id: impl Fn(usize) -> String,
        time: u64,
    ) -> LibraryWriter<W> {
        let mut folders: Vec<String> = Vec::new();
        for (name, _) in heads {
            let folder = file_name(name, &folders);
            folders.push(folder);
        }
        LibraryWriter {
            zip: ZipWriter::to(out),
            folders,
            icons: heads.iter().map(|(_, icon)| icon.map(str::to_owned)).collect(),
            tools,
            ids: (0..=heads.len()).map(id).collect(),
            time,
            begun: 0,
            files: Vec::new(),
        }
    }

    /// Adds `brush` (by `author`, with `pictures`) to set `set`, after those added before. Sets go
    /// in order: one before the last added to is left out.
    pub fn add(&mut self, set: usize, brush: &Brush, author: &str, pictures: &Pictures<'_>) {
        if set + 1 < self.begun || set >= self.folders.len() {
            return;
        }
        while self.begun <= set {
            self.begin_next();
        }
        let Some(folder) = self.folders.get(set) else { return };
        let stem = file_name(&brush.name, &self.files);
        // a brush file is an archive already: kept as it is, not deflated again
        let file = brushfile::write(brush, author, pictures);
        self.zip.add_ready(&format!("{folder}/{stem}.brush"), Ready::of(&file, false));
        self.files.push(stem);
    }

    /// The library can't be made whole (`e`, what went wrong): [`LibraryWriter::finish`] fails.
    pub fn fail(&mut self, e: std::io::Error) {
        self.zip.fail(e);
    }

    /// The last set begun is ended (its list written) and the next begun.
    fn begin_next(&mut self) {
        self.end_set();
        if let Some(folder) = self.folders.get(self.begun) {
            self.zip.add(&format!("{folder}/{EMPTY}"), b"");
        }
        self.begun += 1;
        self.files.clear();
    }

    /// The last set begun, if any, ended: its list of brushes and its properties.
    fn end_set(&mut self) {
        let Some(s) = self.begun.checked_sub(1) else { return };
        let (Some(folder), Some(id)) = (self.folders.get(s), self.ids.get(s + 1)) else { return };
        let mut properties: Vec<(String, String)> = Vec::new();
        if let Some(icon) = self.icons.get(s).cloned().flatten() {
            properties.push(("icon_preset".to_owned(), icon));
        }
        for (tool, at) in self.tools.into_iter().enumerate() {
            if let Some((_, b)) = at.filter(|&(ts, _)| ts == s)
                && let Some(stem) = self.files.get(b)
            {
                properties.push((format!("default_brush_{tool}"), stem.clone()));
            }
        }
        let listed: Vec<String> = self.files.iter().map(|f| format!("{f}.brush")).collect();
        let prefix = meta_prefix();
        let meta = Meta::write(&listed, &properties, self.time);
        self.zip.add(&format!("{folder}/{prefix}{id}"), &meta);
    }

    /// Ends the library (the sets not begun yet as empty ones, then its list): the writer, or why
    /// the file isn't whole.
    pub fn finish(mut self) -> std::io::Result<W> {
        while self.begun < self.folders.len() {
            self.begin_next();
        }
        self.end_set();
        let properties: Vec<(String, String)> = self
            .tools
            .iter()
            .enumerate()
            .filter_map(|(tool, at)| Some((format!("default_set_{tool}"), self.folders.get(at.as_ref()?.0)?.clone())))
            .collect();
        let top = self.ids.first().cloned().unwrap_or_default();
        let meta = Meta::write(&self.folders, &properties, self.time);
        self.zip.add(&format!("{}{top}", meta_prefix()), &meta);
        self.zip.finish_to()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_list_places_each_line_by_its_order() {
        // sorted lines, each order how far its place is from its line's
        let mut bytes = b"Abstract/3/0\nInking/-1/0\nPainting/0/0\nSketching/-2/1746502217\n\0".to_vec();
        bytes.extend(b"default_set_0\0");
        bytes.extend(7u64.to_le_bytes());
        bytes.extend(8u64.to_le_bytes());
        bytes.extend(b"Painting");
        bytes.extend(b"recent\0");
        bytes.extend(9u64.to_le_bytes());
        bytes.extend(0u64.to_le_bytes());
        let meta = Meta::read(&bytes).unwrap();
        assert_eq!(meta.property("default_set_0").as_deref(), Some("Painting"));
        assert_eq!(meta.property("recent").as_deref(), Some(""));
        let found = names(&["Abstract", "Inking", "Painting", "Sketching", "Unlisted"]);
        let order: Vec<&str> = meta.order(&found).into_iter().map(|i| found[i].as_str()).collect();
        assert_eq!(order, ["Inking", "Sketching", "Painting", "Abstract", "Unlisted"]);
        // written back, it reads the same way
        let wanted = names(&["Inking", "Sketching", "Painting", "Abstract"]);
        let written = Meta::read(&Meta::write(&wanted, &[("icon_preset".into(), "Inking".into())], 5)).unwrap();
        let back: Vec<&str> = written.order(&wanted).into_iter().map(|i| wanted[i].as_str()).collect();
        assert_eq!(back, ["Inking", "Sketching", "Painting", "Abstract"]);
        assert_eq!(written.property("icon_preset").as_deref(), Some("Inking"));
        // a damaged property ends them; the lines still read
        let cut = &bytes[..bytes.len() - 20];
        assert_eq!(Meta::read(cut).unwrap().lines.len(), 4);
    }

    /// A real library, when one is given (`LIBRARY_SAMPLE=path`): it reads, its sets in order.
    #[test]
    fn a_sample_reads() {
        let Ok(path) = std::env::var("LIBRARY_SAMPLE") else { return };
        let bytes = std::fs::read(path).unwrap();
        let names: Vec<String> = ZipReader::new(&bytes).unwrap().names().map(str::to_owned).collect();
        assert!(is_library(names.iter().map(String::as_str)));
        let library = read(&bytes).unwrap();
        for set in &library.sets {
            let brushes: Vec<&str> = set.brushes.iter().take(4).map(|b| b.name.as_str()).collect();
            eprintln!("{} ({:?}, {}): {brushes:?}", set.name, set.icon, set.brushes.len());
        }
        let tool = |t: usize| library.tools[t].map(|(s, b)| (&library.sets[s].name, &library.sets[s].brushes[b].name));
        eprintln!("tools {:?} {:?} {:?}", tool(0), tool(1), tool(2));
    }

    #[test]
    fn file_names_are_clean_and_kept_apart() {
        assert_eq!(file_name("Ink/Wash", &[]), "Ink-Wash");
        assert_eq!(file_name(" .hidden ", &[]), "hidden");
        assert_eq!(file_name("", &[]), "Untitled");
        assert_eq!(file_name("Pen", &names(&["Pen", "pen 2"])), "Pen 3");
    }
}
