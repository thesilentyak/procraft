//! Zip archives: reading entries (stored or deflated) and writing deflated ones. Archives are read
//! from memory or, a big one, from its file an entry at a time (Zip64's larger sizes and counts
//! too). No encryption or multi-part archives.

use std::borrow::Cow;
use std::io::{Read, Seek, SeekFrom};

/// An archive: its entries by name, read from memory or from its file.
pub struct ZipReader<'a> {
    source: Source<'a>,
    entries: Vec<Entry>,
    /// Where each name's (first) entry is.
    by_name: std::collections::HashMap<String, usize>,
}

/// Where an archive's bytes are: in memory, or in a file read a piece at a time (one reader at a
/// time, so an archive can be read from several threads).
enum Source<'a> {
    Memory(&'a [u8]),
    File { file: std::sync::Mutex<std::fs::File>, len: u64 },
}

impl Source<'_> {
    fn len(&self) -> u64 {
        match self {
            Source::Memory(bytes) => bytes.len() as u64,
            Source::File { len, .. } => *len,
        }
    }

    /// The `len` bytes at `at`; none if they aren't all there or can't be read.
    fn get(&self, at: u64, len: usize) -> Option<Cow<'_, [u8]>> {
        match self {
            Source::Memory(bytes) => {
                let at = usize::try_from(at).ok()?;
                bytes.get(at..at.checked_add(len)?).map(Cow::Borrowed)
            }
            Source::File { file, len: whole } => {
                // (never more than the file holds, whatever an entry claims)
                if at.checked_add(len as u64)? > *whole {
                    return None;
                }
                let mut file = file.lock().ok()?;
                file.seek(SeekFrom::Start(at)).ok()?;
                let mut out = vec![0; len];
                file.read_exact(&mut out).ok()?;
                Some(Cow::Owned(out))
            }
        }
    }
}

#[derive(Clone)]
struct Entry {
    name: String,
    method: u16,
    flags: u16,
    compressed: u64,
    size: u64,
    crc: u32,
    local: u64,
    attributes: u32,
}

/// What the directory claims of an entry, for a reader that can't take an archive at its word.
pub struct EntryFacts<'a> {
    pub name: &'a str,
    pub size: usize,
    pub compressed: usize,
    pub encrypted: bool,
    /// A plain file, by its attributes: not a link, a folder or a device.
    pub plain: bool,
}

/// The largest entry read: anything claiming more is refused.
const MAX_ENTRY: usize = 1 << 30;

/// The largest central directory read (64 bytes an entry for a million entries).
const MAX_DIRECTORY: u64 = 64 << 20;

impl<'a> ZipReader<'a> {
    /// The archive in `bytes`, if it is one.
    pub fn new(bytes: &'a [u8]) -> Option<ZipReader<'a>> {
        ZipReader::of(Source::Memory(bytes))
    }

    fn of(source: Source<'a>) -> Option<ZipReader<'a>> {
        let entries = directory(&source)?;
        let mut by_name = std::collections::HashMap::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            by_name.entry(e.name.clone()).or_insert(i);
        }
        Some(ZipReader { source, entries, by_name })
    }

    fn entry(&self, name: &str) -> Option<&Entry> {
        self.by_name.get(name).and_then(|&i| self.entries.get(i))
    }

    /// Every entry's name, folders included.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    /// What the directory claims of each entry, in order, those of a repeated name too.
    pub fn facts(&self) -> impl Iterator<Item = EntryFacts<'_>> {
        self.entries.iter().map(|e| {
            // the kind of file in a Unix mode, and a DOS folder's bit
            let kind = (e.attributes >> 16) & 0o170_000;
            let plain = (kind == 0 || kind == 0o100_000) && e.attributes & 0x10 == 0 && !e.name.ends_with('/');
            let size = usize::try_from(e.size).unwrap_or(usize::MAX);
            let compressed = usize::try_from(e.compressed).unwrap_or(usize::MAX);
            EntryFacts { name: &e.name, size, compressed, encrypted: e.flags & 1 != 0, plain }
        })
    }

    /// Whether each entry's own header names what the directory says it does: two readers of an
    /// archive that doesn't would see different files.
    pub fn names_agree(&self) -> bool {
        self.entries.iter().all(|e| {
            let named = || {
                let header = self.source.get(e.local, 30)?;
                (u32le(&header, 0)? == 0x0403_4B50).then_some(())?;
                let len = usize::from(u16le(&header, 26)?);
                self.source.get(e.local.checked_add(30)?, len)
            };
            named().is_some_and(|n| String::from_utf8_lossy(&n) == e.name)
        })
    }

    /// The size entry `name` says it has once read (`read` gives no more).
    pub fn size(&self, name: &str) -> Option<usize> {
        self.entry(name).and_then(|e| usize::try_from(e.size).ok())
    }

    /// The contents of entry `name`; `None` if it is missing, damaged or of a kind not read.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let e = self.entry(name)?;
        let size = usize::try_from(e.size).ok().filter(|&s| s <= MAX_ENTRY)?;
        let compressed = usize::try_from(e.compressed).ok().filter(|&c| c <= MAX_ENTRY)?;
        let header = self.source.get(e.local, 30)?;
        if u32le(&header, 0)? != 0x0403_4B50 {
            return None;
        }
        let name_len = u64::from(u16le(&header, 26)?);
        let extra = u64::from(u16le(&header, 28)?);
        let raw = self.source.get(e.local.checked_add(30 + name_len + extra)?, compressed)?;
        let data = match e.method {
            0 => raw.into_owned(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(&raw, size).ok()?,
            _ => return None,
        };
        (data.len() == size && crc32(&data) == e.crc).then_some(data)
    }
}

impl ZipReader<'static> {
    /// The archive in the file at `path`, if it is one: its directory read from its end, its
    /// entries each when they are read (a big archive is never held whole).
    pub fn open(path: &std::path::Path) -> Option<ZipReader<'static>> {
        let file = std::fs::File::open(path).ok()?;
        let len = file.metadata().ok()?.len();
        ZipReader::of(Source::File { file: std::sync::Mutex::new(file), len })
    }
}

/// The end record is in the last 64 KiB of an archive (its comment can be that long).
const TAIL: usize = 22 + 0xFFFF;

/// Where the end record is in `tail`, the archive's last [`TAIL`] bytes (or all of it).
fn end_record(tail: &[u8]) -> Option<usize> {
    (0..tail.len().checked_sub(21)?).rev().find(|&i| tail.get(i..i + 4) == Some(&[0x50, 0x4B, 0x05, 0x06]))
}

/// The entries the central directory of the archive in `source` lists.
fn directory(source: &Source<'_>) -> Option<Vec<Entry>> {
    let len = source.len();
    let tail_from = len.saturating_sub(TAIL as u64);
    let tail = source.get(tail_from, usize::try_from(len - tail_from).ok()?)?;
    let end = end_record(&tail)?;
    let mut end_at = tail_from + end as u64;
    let mut count = u64::from(u16le(&tail, end + 10)?);
    let mut at = u64::from(u32le(&tail, end + 16)?);
    // one too big for these fields has Zip64's
    if (count == 0xFFFF || at == 0xFFFF_FFFF)
        && let Some((wide_count, wide_at, record_at)) = zip64_end(source, end_at)
    {
        (count, at, end_at) = (wide_count, wide_at, record_at);
    }
    // the directory runs from where it starts to the end record
    let size = end_at.checked_sub(at).filter(|&s| s <= MAX_DIRECTORY)?;
    let bytes = source.get(at, usize::try_from(size).ok()?)?;
    central_entries(&bytes, 0, usize::try_from(count).ok()?)
}

/// Zip64's end record, from its locator just before the end record at `end_at`: how many entries
/// there are, where the directory starts and where the record is; none if there is none.
fn zip64_end(source: &Source<'_>, end_at: u64) -> Option<(u64, u64, u64)> {
    let locator = source.get(end_at.checked_sub(20)?, 20)?;
    (u32le(&locator, 0)? == 0x0706_4B50).then_some(())?;
    let record_at = u64le(&locator, 8)?;
    let record = source.get(record_at, 56)?;
    (u32le(&record, 0)? == 0x0606_4B50).then_some(())?;
    Some((u64le(&record, 32)?, u64le(&record, 48)?, record_at))
}

/// The `count` entries of the central directory at `at` in `bytes`.
fn central_entries(bytes: &[u8], mut at: usize, count: usize) -> Option<Vec<Entry>> {
    let mut entries = Vec::with_capacity(count.min(65_536));
    for _ in 0..count {
        if u32le(bytes, at)? != 0x0201_4B50 {
            return None;
        }
        let flags = u16le(bytes, at + 8)?;
        let method = u16le(bytes, at + 10)?;
        let crc = u32le(bytes, at + 16)?;
        let mut compressed = u64::from(u32le(bytes, at + 20)?);
        let mut size = u64::from(u32le(bytes, at + 24)?);
        let name_len = usize::from(u16le(bytes, at + 28)?);
        let extra = usize::from(u16le(bytes, at + 30)?);
        let comment = usize::from(u16le(bytes, at + 32)?);
        let attributes = u32le(bytes, at + 38)?;
        let mut local = u64::from(u32le(bytes, at + 42)?);
        let name = String::from_utf8_lossy(bytes.get(at + 46..at + 46 + name_len)?).into_owned();
        // sizes and places too big for their fields are in a Zip64 extra field, in this order
        let full = [size, compressed, local].map(|v| v == 0xFFFF_FFFF);
        if full.contains(&true) {
            let wide = zip64_field(bytes.get(at + 46 + name_len..at + 46 + name_len + extra)?)?;
            let mut values = wide.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c));
            for (is_full, value) in full.iter().zip([&mut size, &mut compressed, &mut local]) {
                if *is_full {
                    *value = values.next()?;
                }
            }
        }
        entries.push(Entry { name, method, flags, compressed, size, crc, local, attributes });
        at += 46 + name_len + extra + comment;
    }
    Some(entries)
}

/// The data of the Zip64 field among an entry's `extra` fields.
fn zip64_field(mut extra: &[u8]) -> Option<&[u8]> {
    while extra.len() >= 4 {
        let (id, len) = (u16le(extra, 0)?, usize::from(u16le(extra, 2)?));
        let data = extra.get(4..4 + len)?;
        if id == 0x0001 {
            return Some(data);
        }
        extra = extra.get(4 + len..)?;
    }
    None
}

/// The names in the archive at `path`, read from its end (the central directory) alone: a big
/// archive is told by what it holds without reading all of it.
pub fn names_in_file(path: &std::path::Path) -> Option<Vec<String>> {
    Some(ZipReader::open(path)?.names().map(str::to_owned).collect())
}

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

fn u64le(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at.checked_add(8)?)?.try_into().ok()?))
}

/// Writes an archive entry by entry: into memory ([`ZipWriter::new`]), or into a writer as it
/// goes ([`ZipWriter::to`]), so that a big archive is never all in memory.
#[derive(Default)]
pub struct ZipWriter<W: std::io::Write = Vec<u8>> {
    out: W,
    /// How many bytes are written.
    at: u64,
    central: Vec<u8>,
    count: u16,
    /// The first write that failed, or what its caller said went wrong: the archive isn't whole.
    failed: Option<std::io::Error>,
}

/// An entry made ready to add with [`ZipWriter::add_ready`] (on any thread, for adding in order):
/// its data's checksum and size, and how it is written.
pub struct Ready {
    crc: u32,
    size: usize,
    method: u16,
    body: Vec<u8>,
}

impl Ready {
    /// `data` deflated if that makes it smaller, and `deflate` is asked (it isn't, for data
    /// already compressed that won't shrink); else stored.
    pub fn of(data: &[u8], deflate: bool) -> Ready {
        let (crc, size) = (crc32(data), data.len());
        if deflate {
            let deflated = miniz_oxide::deflate::compress_to_vec(data, 6);
            if deflated.len() < size {
                return Ready { crc, size, method: 8, body: deflated };
            }
        }
        Ready { crc, size, method: 0, body: data.to_vec() }
    }
}

impl ZipWriter {
    pub fn new() -> ZipWriter {
        ZipWriter::default()
    }

    /// The finished archive.
    pub fn finish(mut self) -> Vec<u8> {
        self.end();
        self.out
    }
}

impl<W: std::io::Write> ZipWriter<W> {
    /// An archive written into `out` as its entries are added.
    pub fn to(out: W) -> ZipWriter<W> {
        ZipWriter { out, at: 0, central: Vec::new(), count: 0, failed: None }
    }

    /// The archive can't be made whole (`e`, what went wrong): [`ZipWriter::finish_to`] fails.
    pub fn fail(&mut self, e: std::io::Error) {
        self.failed.get_or_insert(e);
    }

    /// Ends an archive written into a writer: the writer, or why the archive isn't whole.
    pub fn finish_to(mut self) -> std::io::Result<W> {
        self.end();
        match self.failed {
            Some(e) => Err(e),
            None => Ok(self.out),
        }
    }

    /// `bytes` written next (nothing more once a write failed).
    fn put(&mut self, bytes: &[u8]) {
        if self.failed.is_none()
            && let Err(e) = self.out.write_all(bytes)
        {
            self.failed = Some(e);
        }
        self.at += bytes.len() as u64;
    }

    /// Adds `data` as entry `name`, deflated (or stored, if deflating doesn't make it smaller).
    pub fn add(&mut self, name: &str, data: &[u8]) {
        self.add_ready(name, Ready::of(data, true));
    }

    /// Adds an entry made ready (perhaps on another thread) as entry `name`.
    pub fn add_ready(&mut self, name: &str, ready: Ready) {
        self.entry(name, ready.crc, ready.size, ready.method, &ready.body, 0);
    }

    /// Adds `data` as entry `name` stored as it is, starting at a multiple of `align` bytes into
    /// the archive (as USDZ packages ask: the local header padded with an extra field).
    pub fn add_aligned(&mut self, name: &str, data: &[u8], align: usize) {
        let align = align.max(1);
        let at = self.at as usize + 30 + name.len();
        let mut pad = (align - at % align) % align;
        // an extra field needs four bytes for its id and size
        if pad > 0 && pad < 4 {
            pad += align;
        }
        self.entry(name, crc32(data), data.len(), 0, data, pad);
    }

    /// An entry whose data (`size` bytes, checksum `crc`) is written as `body` by `method`.
    fn entry(&mut self, name: &str, crc: u32, size: usize, method: u16, body: &[u8], pad: usize) {
        let local = self.at as u32;
        let name = name.as_bytes();
        let header = |sig: u32, central: bool| {
            let mut h = Vec::with_capacity(46 + name.len());
            h.extend_from_slice(&sig.to_le_bytes());
            if central {
                h.extend_from_slice(&20u16.to_le_bytes()); // made by
            }
            h.extend_from_slice(&20u16.to_le_bytes()); // needed
            h.extend_from_slice(&0x0800u16.to_le_bytes()); // names are UTF-8
            h.extend_from_slice(&method.to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes()); // time
            h.extend_from_slice(&0x0021u16.to_le_bytes()); // date: 1980-01-01
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(body.len() as u32).to_le_bytes());
            h.extend_from_slice(&(size as u32).to_le_bytes());
            h.extend_from_slice(&(name.len() as u16).to_le_bytes());
            let extra = if central { 0 } else { pad as u16 };
            h.extend_from_slice(&extra.to_le_bytes());
            if central {
                h.extend_from_slice(&0u16.to_le_bytes()); // comment
                h.extend_from_slice(&0u16.to_le_bytes()); // disk
                h.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
                h.extend_from_slice(&0u32.to_le_bytes()); // external attributes
                h.extend_from_slice(&local.to_le_bytes());
            }
            h.extend_from_slice(name);
            if !central && pad >= 4 {
                // padding: an extra field of our own id, its size what is left
                h.extend_from_slice(&0x1986u16.to_le_bytes());
                h.extend_from_slice(&((pad - 4) as u16).to_le_bytes());
                h.resize(h.len() + pad - 4, 0);
            }
            h
        };
        let local_header = header(0x0403_4B50, false);
        self.put(&local_header);
        self.put(body);
        self.central.extend_from_slice(&header(0x0201_4B50, true));
        self.count = self.count.saturating_add(1);
    }

    /// The central directory and its end, after the entries.
    fn end(&mut self) {
        let start = self.at as u32;
        let central = std::mem::take(&mut self.central);
        let size = central.len() as u32;
        self.put(&central);
        let mut tail = Vec::with_capacity(22);
        tail.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
        tail.extend_from_slice(&[0; 4]);
        tail.extend_from_slice(&self.count.to_le_bytes());
        tail.extend_from_slice(&self.count.to_le_bytes());
        tail.extend_from_slice(&size.to_le_bytes());
        tail.extend_from_slice(&start.to_le_bytes());
        tail.extend_from_slice(&0u16.to_le_bytes());
        self.put(&tail);
    }
}

/// The CRC-32 zip uses.
pub fn crc32(data: &[u8]) -> u32 {
    const fn table() -> [u32; 256] {
        let mut t = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            #[allow(clippy::indexing_slicing)] // const evaluation: i is below 256
            {
                t[i] = c;
            }
            i += 1;
        }
        t
    }
    const TABLE: [u32; 256] = table();
    !data.iter().fold(!0u32, |c, &b| TABLE.get(usize::from((c as u8) ^ b)).copied().unwrap_or(0) ^ (c >> 8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip() {
        let mut w = ZipWriter::new();
        let big: Vec<u8> = (0..100_000u32).map(|i| (i % 7) as u8).collect();
        w.add("Document.archive", b"bplist00 not really");
        w.add("A1/0~0.lz4", &big);
        w.add("QuickLook/Thumbnail.png", &[]);
        let bytes = w.finish();
        let r = ZipReader::new(&bytes).unwrap();
        assert_eq!(r.names().collect::<Vec<_>>(), ["Document.archive", "A1/0~0.lz4", "QuickLook/Thumbnail.png"]);
        assert_eq!(r.read("A1/0~0.lz4").as_deref(), Some(big.as_slice()));
        assert_eq!(r.read("QuickLook/Thumbnail.png").as_deref(), Some(&[][..]));
        assert_eq!(r.read("missing"), None);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn a_files_names_are_read_from_its_end() {
        let dir = std::env::temp_dir().join(format!("procraft-zip-names-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut w = ZipWriter::new();
        // a big entry the names are read past
        let big: Vec<u8> = (0..300_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
        w.add("Document.archive", b"bplist00");
        w.add("layer/0~0.chunk", &big);
        w.add("QuickLook/Thumbnail.png", &[]);
        let zip = dir.join("a.zip");
        std::fs::write(&zip, w.finish()).unwrap();
        let other = dir.join("b.txt");
        std::fs::write(&other, b"PK but no archive").unwrap();
        let names = names_in_file(&zip);
        let none = (names_in_file(&other), names_in_file(&dir.join("missing")));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(names.unwrap(), ["Document.archive", "layer/0~0.chunk", "QuickLook/Thumbnail.png"]);
        assert_eq!(none, (None, None));
    }

    #[test]
    fn an_entry_says_what_kind_of_thing_it_is() {
        let mut w = ZipWriter::new();
        w.add("plain.txt", b"words");
        w.add("folder/", b"");
        let mut bytes = w.finish();
        let plain = |bytes: &[u8]| {
            let r = ZipReader::new(bytes).unwrap();
            r.facts().map(|f| f.plain).collect::<Vec<_>>()
        };
        assert_eq!(plain(&bytes), [true, false]);
        // the first central entry made a link, as Unix's zip writes one
        let central = bytes.windows(4).position(|w| w == [0x50, 0x4B, 0x01, 0x02]).unwrap();
        bytes[central + 38..central + 42].copy_from_slice(&(0o120_777u32 << 16).to_le_bytes());
        assert_eq!(plain(&bytes), [false, false]);
    }

    #[test]
    fn names_agree_only_where_headers_and_directory_do() {
        let mut w = ZipWriter::new();
        w.add("aa.txt", b"words");
        let mut bytes = w.finish();
        assert!(ZipReader::new(&bytes).unwrap().names_agree());
        // the local header's name changed, the directory's not
        bytes[30] = b'z';
        assert!(!ZipReader::new(&bytes).unwrap().names_agree());
    }

    #[test]
    fn an_archive_in_a_file_reads_as_one_in_memory() {
        let dir = std::env::temp_dir().join(format!("procraft-zip-file-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut w = ZipWriter::new();
        let noise: Vec<u8> = (0..200_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
        w.add("brushset.plist", b"<plist/>");
        w.add("one/Shape.png", &noise);
        w.add_ready("one/Grain.png", Ready::of(&noise, false));
        let bytes = w.finish();
        let path = dir.join("set.brushset");
        std::fs::write(&path, &bytes).unwrap();
        let file = ZipReader::open(&path).unwrap();
        let memory = ZipReader::new(&bytes).unwrap();
        assert!(file.names().eq(memory.names()));
        assert!(file.names_agree());
        for name in ["brushset.plist", "one/Shape.png", "one/Grain.png", "missing"] {
            assert_eq!(file.read(name), memory.read(name), "{name}");
        }
        assert_eq!(file.read("one/Grain.png").as_deref(), Some(noise.as_slice()));
        assert!(ZipReader::open(&dir.join("missing")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An archive of one stored entry `name` holding `data`, its sizes, place and count in Zip64's
    /// fields and records alone.
    fn zip64(name: &str, data: &[u8]) -> Vec<u8> {
        let crc = crc32(data);
        let mut out = Vec::new();
        out.extend_from_slice(&0x0403_4B50u32.to_le_bytes());
        out.extend_from_slice(&[45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        let directory = out.len() as u64;
        out.extend_from_slice(&0x0201_4B50u32.to_le_bytes());
        out.extend_from_slice(&[45, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&[0xFF; 8]);
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&28u16.to_le_bytes());
        out.extend_from_slice(&[0; 6]);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&[0xFF; 4]);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        let size = out.len() as u64 - directory;
        let record = out.len() as u64;
        out.extend_from_slice(&0x0606_4B50u32.to_le_bytes());
        out.extend_from_slice(&44u64.to_le_bytes());
        out.extend_from_slice(&[45, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&directory.to_le_bytes());
        out.extend_from_slice(&0x0706_4B50u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&record.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&0x0605_4B50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&[0xFF; 4]);
        out.extend_from_slice(&[0xFF; 8]);
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn a_zip64_archive_s_entries_are_read_by_their_wide_fields() {
        let bytes = zip64("Brush.archive", b"bplist00 a brush");
        let r = ZipReader::new(&bytes).unwrap();
        assert_eq!(r.names().collect::<Vec<_>>(), ["Brush.archive"]);
        assert_eq!(r.size("Brush.archive"), Some(16));
        assert_eq!(r.read("Brush.archive").as_deref(), Some(&b"bplist00 a brush"[..]));
        assert!(r.names_agree());
    }

    #[test]
    fn damaged_archives_read_as_nothing() {
        assert!(ZipReader::new(b"PK nothing here").is_none());
        let mut w = ZipWriter::new();
        w.add("a", &[1, 2, 3, 4, 5, 6, 7, 8].repeat(100));
        let mut bytes = w.finish();
        // a flipped byte in the data fails the check
        if let Some(b) = bytes.get_mut(40) {
            *b ^= 0xFF;
        }
        let r = ZipReader::new(&bytes).unwrap();
        assert_eq!(r.read("a"), None);
    }
}
