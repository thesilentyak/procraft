//! Reading layered PSD (and large-document PSB) files as artworks: their layers with names,
//! opacity, visibility, blend modes, clipping and locks; groups; layer masks; text layers as
//! editable text; the resolution and colour profile. Grey, RGB and CMYK files of 8 or 16 bits a
//! channel are read.
//!
//! What comes in is each layer's own pixels: effects, vector masks (beyond the mask the file
//! renders from one), adjustments and fill opacity are left out, and so is a layer with no pixels
//! on the canvas. A bottom layer of one opaque colour over the whole canvas becomes the canvas's
//! background colour; otherwise the background is hidden. A file with no layers to bring, or whose
//! layers can't be read, comes in as one layer of its flattened picture.
//!
//! A file is never read whole ([`Source`]): its layers' records first, then each layer's pixels as
//! they are asked for, and of those only the part on the canvas, so what reading holds at once is
//! a layer or two whatever the file's size and number of layers. Damage is read past: a channel cut
//! short, or that doesn't unpack, comes in as far as it reads.

mod text;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use miniz_oxide::inflate::stream::{InflateState, inflate};
use miniz_oxide::{DataFormat, MZError, MZFlush, MZStatus};

use crate::cms::{CanvasIcc, IccProfile, Space};
use crate::document::{
    BlendMode, Document, Layer, LayerId, LayerKind, LayerText, MAX_CANVAS_EDGE, canvas_edge_most, layer_cap_for,
    layer_room_for,
};
use crate::native::Imported;
use crate::profile::Profile;

/// Why a PSD couldn't be read.
#[derive(Debug, PartialEq, Eq)]
pub enum PsdError {
    NotAPsd,
    /// A kind of picture not read here (bitmap, indexed, Lab, 32 bits a channel...).
    Unsupported(&'static str),
    Damaged(&'static str),
    /// Larger than a canvas can be on this device: its sides, or a layer's memory beyond what the
    /// layers may take.
    TooLarge,
    /// More layers than a canvas this size holds (it holds this many).
    TooManyLayers(usize),
}

/// Whether `bytes` start as a PSD or PSB does.
pub fn is_psd(bytes: &[u8]) -> bool {
    bytes.starts_with(b"8BPS")
}

/// Where a PSD is read from: bytes in memory, or a file read a part at a time ([`FileSource`]).
pub trait Source: Sync {
    /// How many bytes there are.
    fn size(&self) -> u64;
    /// Fills `buf` with the bytes from `at`: false if they run past the end or can't be read.
    fn read_at(&self, at: u64, buf: &mut [u8]) -> bool;
}

impl Source for [u8] {
    fn size(&self) -> u64 {
        self.len() as u64
    }

    fn read_at(&self, at: u64, buf: &mut [u8]) -> bool {
        let from = usize::try_from(at).ok();
        match from.and_then(|f| self.get(f..f.checked_add(buf.len())?)) {
            Some(part) => {
                buf.copy_from_slice(part);
                true
            }
            None => false,
        }
    }
}

/// A file read a part at a time where it is needed, never whole.
pub struct FileSource {
    file: std::fs::File,
    size: u64,
    /// One read at a time where the system has no reading at a place.
    #[cfg(not(any(unix, windows)))]
    turn: std::sync::Mutex<()>,
}

impl FileSource {
    pub fn open(path: &std::path::Path) -> std::io::Result<FileSource> {
        let file = std::fs::File::open(path)?;
        let size = file.metadata()?.len();
        Ok(FileSource {
            file,
            size,
            #[cfg(not(any(unix, windows)))]
            turn: std::sync::Mutex::new(()),
        })
    }
}

impl Source for FileSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&self, at: u64, buf: &mut [u8]) -> bool {
        if at.checked_add(buf.len() as u64).is_none_or(|end| end > self.size) {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;
            self.file.read_exact_at(buf, at).is_ok()
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileExt;
            let mut done = 0;
            while let Some(rest) = buf.get_mut(done..).filter(|r| !r.is_empty()) {
                match self.file.seek_read(rest, at + done as u64) {
                    Ok(0) => return false,
                    Ok(n) => done += n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => return false,
                }
            }
            true
        }
        #[cfg(not(any(unix, windows)))]
        {
            use std::io::{Read, Seek, SeekFrom};
            let _turn = self.turn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut file = &self.file;
            file.seek(SeekFrom::Start(at)).is_ok() && file.read_exact(buf).is_ok()
        }
    }
}

/// What a file is, as far as reading its pixels goes.
#[derive(Clone, Copy)]
struct Format {
    width: u32,
    height: u32,
    /// Bytes a sample: 1, 2, or 4 (32-bit floats, linear light); 0 for a bitmap's bits.
    sample: usize,
    colour: Colour,
    /// An indexed file's colours (its colour mode data).
    palette: [[u8; 3]; 256],
    /// PSB: some lengths take eight bytes.
    big: bool,
}

impl Format {
    fn canvas(&self) -> (usize, usize) {
        (self.width as usize, self.height as usize)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Colour {
    /// One bit a pixel, set for black.
    Bitmap,
    /// Grey, and a duotone's grey (its inks left out).
    Grey,
    /// A colour from the file's table of 256.
    Indexed,
    Rgb,
    Cmyk,
    /// CIE L*a*b* (D50), brought to sRGB.
    Lab,
}

impl Colour {
    fn channels(self) -> usize {
        match self {
            Colour::Bitmap | Colour::Grey | Colour::Indexed => 1,
            Colour::Rgb | Colour::Lab => 3,
            Colour::Cmyk => 4,
        }
    }
}

/// What a file's colour `mode` and `depth` make of its samples: the colour, the bytes a sample
/// (0: a bitmap's bits), and an indexed file's colours from its colour mode `data`; else what
/// isn't read (multichannel files, other depths).
fn format_of(mode: u16, depth: u16, data: &[u8]) -> Result<(Colour, usize, [[u8; 3]; 256]), PsdError> {
    let colour = match mode {
        0 => Colour::Bitmap,
        1 | 8 => Colour::Grey,
        2 => Colour::Indexed,
        3 => Colour::Rgb,
        4 => Colour::Cmyk,
        9 => Colour::Lab,
        _ => return Err(PsdError::Unsupported("colour mode")),
    };
    let sample = match (colour, depth) {
        (Colour::Bitmap, 1) => 0,
        (Colour::Bitmap, _) => return Err(PsdError::Unsupported("bit depth")),
        (_, 8) => 1,
        (_, 16) => 2,
        (_, 32) => 4,
        _ => return Err(PsdError::Unsupported("bit depth")),
    };
    let mut palette = [[0u8; 3]; 256];
    if colour == Colour::Indexed {
        // the reds, then the greens, then the blues
        let table = data.get(..768).ok_or(PsdError::Damaged("colour data"))?;
        for (i, c) in palette.iter_mut().enumerate() {
            *c = [0, 256, 512].map(|k| table.get(k + i).copied().unwrap_or(0));
        }
    }
    Ok((colour, sample, palette))
}

/// Bytes a row of `w` samples of `sample` bytes takes (a bitmap's bits packed eight a byte).
fn row_bytes(w: usize, sample: usize) -> Option<usize> {
    if sample == 0 { Some(w.div_ceil(8)) } else { w.checked_mul(sample) }
}

/// The most of a block of additional information that is read for a layer's name.
const MOST_NAME: u64 = 1 << 17;
/// The most of a text layer's block that is read (more: it stays pixels).
const MOST_TEXT: u64 = 16 << 20;
/// The most of a colour profile that is read.
const MOST_PROFILE: u64 = 16 << 20;

/// A PSD's layers, as they come into a canvas: the document, and each layer's pixels read from the
/// file as they are asked for ([`PsdArtwork::pixels`]).
pub struct PsdArtwork<'s, S: Source + ?Sized> {
    psd: Parsed<'s, S>,
    pub document: Document,
    /// Where each layer with pixels of its own gets them, bottom first.
    sources: Vec<(LayerId, Pixels)>,
    /// The flattened picture, premultiplied, when it is the one layer.
    flat: Option<Vec<u8>>,
    /// The layers' records couldn't be read: the flattened picture came in instead.
    pub salvaged: bool,
}

/// Where a layer's pixels come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pixels {
    /// A record's own.
    Record(usize),
    /// A record's mask's.
    Mask(usize),
    /// The flattened picture.
    Flat,
}

impl<S: Source + ?Sized> PsdArtwork<'_, S> {
    /// The layers that have pixels of their own to read, bottom first (the others start clear).
    pub fn layers_with_pixels(&self) -> Vec<LayerId> {
        self.sources.iter().map(|(id, _)| *id).collect()
    }

    /// Layer `id`'s pixels on the canvas, premultiplied RGBA (a mask's grey in every channel,
    /// opaque), read from the file now; none if it has none (it is clear).
    pub fn pixels(&self, id: LayerId) -> Option<Vec<u8>> {
        let (_, from) = self.sources.iter().find(|(i, _)| *i == id)?;
        match *from {
            Pixels::Record(i) => self.psd.records.get(i).and_then(|r| self.psd.layer_pixels(r)),
            Pixels::Mask(i) => self.psd.records.get(i).and_then(|r| self.psd.mask_pixels(r)),
            Pixels::Flat => self.flat.clone(),
        }
    }

    /// The flattened picture the file carries (straight RGBA, the canvas's size), if it does.
    pub fn flattened(&self) -> Option<Vec<u8>> {
        merged(self.psd.src, self.psd.flat, &self.psd.format, self.psd.channels)
    }

    /// The flattened picture the file carries on white, made small to fit `side` (read a row at a
    /// time, never held whole): its size and opaque RGBA; none if it carries none.
    pub fn thumbnail(&self, side: u32) -> Option<(u32, u32, Vec<u8>)> {
        merged_thumbnail(self.psd.src, self.psd.flat, &self.psd.format, self.psd.channels, side)
    }
}

/// Opens a PSD or PSB from `src`: its document, its layers' pixels read as they are asked for.
pub fn open<S: Source + ?Sized>(src: &S) -> Result<PsdArtwork<'_, S>, PsdError> {
    let psd = parse(src)?;
    let (w, h) = (psd.format.width, psd.format.height);
    // a canvas this device can't hold (its sides, or not one layer's worth of memory for it) is
    // turned away whole, as its layers would be on opening it
    let most = canvas_edge_most();
    if w > most || h > most || layer_room_for(w, h) == 0 {
        return Err(PsdError::TooLarge);
    }
    open_within(psd, layer_cap_for(w, h))
}

/// [`open`] past the device's checks: `cap` layers with pixels at most, or none of it comes in.
fn open_within<S: Source + ?Sized>(psd: Parsed<'_, S>, cap: usize) -> Result<PsdArtwork<'_, S>, PsdError> {
    let f = psd.format;
    let (w, h) = (f.width, f.height);
    // too many layers: the whole file turned away (a bottom layer that is the background doesn't
    // count, so only that one is looked at before)
    let counted = plan(&psd.records, &f, false).pixels.len();
    if counted > cap.saturating_add(1) {
        return Err(PsdError::TooManyLayers(cap));
    }
    let background = psd.records.first().and_then(|r| psd.uniform(r));
    let mut made = plan(&psd.records, &f, background.is_some());
    if made.pixels.len() > cap {
        return Err(PsdError::TooManyLayers(cap));
    }
    // nothing to bring, or the records unread: the flattened picture
    let mut flat = None;
    if made.layers.is_empty() {
        let picture = merged(psd.src, psd.flat, &f, psd.channels).ok_or(PsdError::Damaged("picture"))?;
        let id = LayerId(1);
        made = Plan {
            layers: vec![paint_layer(id, "Layer 1".to_owned(), None)],
            pixels: vec![(id, Pixels::Flat)],
            top: Some(id),
        };
        flat = Some(premultiplied(&picture));
    }
    let Plan { layers, pixels, top } = made;
    let mut document = Document::from_layers(w, h, layers, top.unwrap_or(LayerId(1)));
    // white and hidden unless the bottom layer was all one colour
    document.background = background.map_or([1.0; 3], |c| c.map(|v| f32::from(v) / 255.0));
    document.background_visible = background.is_some();
    if let Some(dpi) = psd.resources.dpi {
        document.dpi = dpi;
    }
    if let Some(icc) = psd.resources.icc.as_deref().and_then(IccProfile::parse) {
        match Profile::from_name(&icc.name) {
            Some(builtin) => document.profile = builtin,
            None if icc.space == Space::Rgb => {
                document.profile = if icc.name.contains("P3") { Profile::DisplayP3 } else { Profile::Srgb };
                document.icc = Some(CanvasIcc(Arc::new(icc)));
            }
            None => {}
        }
    }
    // a layer the document gained (when only groups came) starts clear
    let document = document.repaired();
    let sources: Vec<(LayerId, Pixels)> = pixels.into_iter().filter(|(id, _)| document.layer(*id).is_some()).collect();
    let salvaged = psd.records_damaged;
    Ok(PsdArtwork { psd, document, sources, flat, salvaged })
}

/// Reads a PSD or PSB in memory whole: its document and every layer's pixels.
pub fn read(bytes: &[u8]) -> Result<Imported, PsdError> {
    let art = open(bytes)?;
    let size = art.document.width() as usize * art.document.height() as usize * 4;
    let ids: Vec<LayerId> = art.document.pixel_layer_ids().collect();
    let pixels = ids.into_iter().map(|id| (id, art.pixels(id).unwrap_or_else(|| vec![0; size]))).collect();
    let PsdArtwork { document, .. } = art;
    Ok(Imported {
        document,
        pixels,
        name: "Untitled Artwork".to_owned(),
        author: String::new(),
        strokes: 0,
        tracked_seconds: 0.0,
        reference: crate::reference_window::ReferenceWindow::default(),
        reference_picture: None,
    })
}

/// The flattened picture a PSD carries (straight RGBA, the canvas's size), if it has one.
pub fn flattened(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let head = head(bytes).ok()?;
    let f = head.format;
    merged(bytes, head.flat, &f, head.channels).map(|rgba| (f.width, f.height, rgba))
}

/// What the image resources say.
#[derive(Default)]
struct Resources {
    dpi: Option<f32>,
    icc: Option<Vec<u8>>,
}

fn resources<S: Source + ?Sized>(mut c: Cursor<'_, S>) -> Resources {
    let mut out = Resources::default();
    while let Some(sig) = c.array::<4>() {
        // the usual signature, and the others some writers use
        if ![*b"8BIM", *b"MeSa", *b"AgHg", *b"PHUT", *b"DCSR"].contains(&sig) {
            break;
        }
        let Some(id) = c.u16() else { break };
        // a name, padded to an even length with its length byte
        let Some(name) = c.u8() else { break };
        if c.skip(u64::from(name) + u64::from(name % 2 == 0)).is_none() {
            break;
        }
        let Some(size) = c.u32().map(u64::from) else { break };
        let Some(mut data) = c.sub(size) else { break };
        if size % 2 == 1 {
            let _ = c.skip(1);
        }
        match id {
            // horizontal resolution, 16.16, per inch (unit 1) or per centimetre (2)
            0x03ED => {
                if let (Some(fixed), Some(unit)) = (data.u32(), data.u16()) {
                    let per = fixed as f32 / 65536.0;
                    let dpi = if unit == 2 { per * 2.54 } else { per };
                    if dpi.is_finite() && dpi >= 1.0 {
                        out.dpi = Some(dpi.min(9999.0));
                    }
                }
            }
            0x040F if size <= MOST_PROFILE => out.icc = data.bytes(size),
            _ => {}
        }
    }
    out
}

/// A rectangle as a file gives it.
#[derive(Clone, Copy, Debug, Default)]
struct Rect {
    top: i32,
    left: i32,
    bottom: i32,
    right: i32,
}

impl Rect {
    fn width(self) -> usize {
        span(self.left, self.right)
    }

    fn height(self) -> usize {
        span(self.top, self.bottom)
    }

    /// Where it and the canvas meet; none if nowhere.
    fn on_canvas(self, f: &Format) -> Option<Window> {
        let x0 = i64::from(self.left).max(0);
        let y0 = i64::from(self.top).max(0);
        let x1 = (i64::from(self.left) + self.width() as i64).min(i64::from(f.width));
        let y1 = (i64::from(self.top) + self.height() as i64).min(i64::from(f.height));
        (x1 > x0 && y1 > y0).then_some(Window { x0: x0 as usize, y0: y0 as usize, x1: x1 as usize, y1: y1 as usize })
    }
}

/// A rectangle's extent along one side (none if back to front or too big).
fn span(from: i32, to: i32) -> usize {
    usize::try_from(i64::from(to) - i64::from(from)).ok().filter(|&n| n <= 1 << 20).unwrap_or(0)
}

/// Part of the canvas: columns `x0..x1` of rows `y0..y1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Window {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl Window {
    fn width(self) -> usize {
        self.x1.saturating_sub(self.x0)
    }

    fn height(self) -> usize {
        self.y1.saturating_sub(self.y0)
    }

    /// The whole canvas.
    fn whole(self, f: &Format) -> bool {
        (self.x0, self.y0, self.x1, self.y1) == (0, 0, f.width as usize, f.height as usize)
    }
}

/// A layer as the file has it.
struct Record {
    rect: Rect,
    /// Each channel (0.. colours, -1 alpha, -2 mask) and where its data is.
    channels: Vec<Channel>,
    blend: [u8; 4],
    opacity: u8,
    clipped: bool,
    hidden: bool,
    /// Its flags lock its transparency.
    flagged_lock: bool,
    mask: Option<MaskRect>,
    name: String,
    /// A group's marker: 1 open, 2 closed, 3 the end of one (below its layers).
    section: Option<u32>,
    /// What is locked: bit 0 transparency, bit 1 pixels, bit 2 position; past 6 all of it.
    protection: u32,
    /// A text layer's text, if it can be edited as text.
    text: Option<LayerText>,
}

/// A channel: its id, and where its data is in the file (its compression, then its samples).
#[derive(Clone, Copy, Debug)]
struct Channel {
    id: i16,
    at: u64,
    len: u64,
}

impl Record {
    /// Locked against changes: its pixels or position, or all of it.
    fn locked(&self) -> bool {
        self.protection & 0b110 != 0 || self.protection > 6
    }

    /// Its transparency locked.
    fn alpha_locked(&self) -> bool {
        self.protection & 1 != 0 || self.protection > 6 || self.flagged_lock
    }

    fn channel(&self, id: i16) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    /// Where its pixels lie on the canvas: none if no part of them does, or one of its colour
    /// channels is missing.
    fn window(&self, f: &Format) -> Option<Window> {
        let colours = (0..f.colour.channels()).all(|c| i16::try_from(c).is_ok_and(|c| self.channel(c).is_some()));
        if colours { self.rect.on_canvas(f) } else { None }
    }

    /// Its mask, if it has one with pixels.
    fn mask(&self) -> Option<MaskRect> {
        self.mask.filter(|m| m.rect.width() * m.rect.height() > 0 && self.channel(-2).is_some())
    }
}

#[derive(Clone, Copy)]
struct MaskRect {
    rect: Rect,
    /// The mask outside its rectangle.
    outside: u8,
    /// Turned off: kept, hidden.
    hidden: bool,
}

/// A PSD's parts, as read before any pixels: its format, its layers' records, and where its
/// flattened picture is.
struct Parsed<'s, S: Source + ?Sized> {
    src: &'s S,
    format: Format,
    channels: u16,
    resources: Resources,
    records: Vec<Record>,
    /// The records couldn't be read.
    records_damaged: bool,
    /// Where the flattened picture starts.
    flat: u64,
}

/// A PSD's header and image resources, and where its layers' section and flattened picture are.
struct Head {
    format: Format,
    channels: u16,
    resources: Resources,
    /// The layers' section, from and to.
    section: (u64, u64),
    flat: u64,
}

fn head<S: Source + ?Sized>(src: &S) -> Result<Head, PsdError> {
    let damaged = PsdError::Damaged;
    let mut c = Cursor::new(src, 0, src.size());
    if c.array::<4>() != Some(*b"8BPS") {
        return Err(PsdError::NotAPsd);
    }
    let big = match c.u16().ok_or(damaged("header"))? {
        1 => false,
        2 => true,
        _ => return Err(damaged("version")),
    };
    c.skip(6).ok_or(damaged("header"))?;
    let channels = c.u16().ok_or(damaged("header"))?;
    let height = c.u32().ok_or(damaged("header"))?;
    let width = c.u32().ok_or(damaged("header"))?;
    let depth = c.u16().ok_or(damaged("header"))?;
    let mode = c.u16().ok_or(damaged("header"))?;
    if width == 0 || height == 0 {
        return Err(damaged("size"));
    }
    if width > MAX_CANVAS_EDGE || height > MAX_CANVAS_EDGE {
        return Err(PsdError::TooLarge);
    }
    // colour mode data: an indexed file's table (a duotone's inks are left out)
    let n = u64::from(c.u32().ok_or(damaged("colour data"))?);
    let mut data = c.sub(n).ok_or(damaged("colour data"))?;
    let table = if mode == 2 { data.bytes(n.min(768)).unwrap_or_default() } else { Vec::new() };
    let (colour, sample, palette) = format_of(mode, depth, &table)?;
    let format = Format { width, height, sample, colour, palette, big };
    let n = u64::from(c.u32().ok_or(damaged("resources"))?);
    let resources = resources(c.sub(n).ok_or(damaged("resources"))?);
    // the layers' section; cut short, what of it there is (and no flattened picture after it)
    let n = c.len(big).ok_or(damaged("layers"))?;
    let from = c.at;
    let to = from.saturating_add(n).min(c.end);
    let flat = from.saturating_add(n);
    Ok(Head { format, channels, resources, section: (from, to), flat })
}

fn parse<S: Source + ?Sized>(src: &S) -> Result<Parsed<'_, S>, PsdError> {
    let Head { format, channels, resources, section, flat } = head(src)?;
    let (records, records_damaged) = match layers_of(Cursor::new(src, section.0, section.1), &format) {
        Ok(records) => (records, false),
        Err(_) => (Vec::new(), true),
    };
    Ok(Parsed { src, format, channels, resources, records, records_damaged, flat })
}

/// The layers' records from the layers' section, bottom first: in its layer info, or (16- and
/// 32-bit files) in a block after the global mask.
fn layers_of<S: Source + ?Sized>(mut section: Cursor<'_, S>, f: &Format) -> Result<Vec<Record>, PsdError> {
    if section.remaining() == 0 {
        return Ok(Vec::new());
    }
    let n = section.len(f.big).ok_or(PsdError::Damaged("layers"))?;
    // cut short: what of it there is
    let info = section.sub(n.min(section.remaining())).ok_or(PsdError::Damaged("layers"))?;
    let mut records = if n > 0 { layer_info(info, f)? } else { Vec::new() };
    // the global mask, then blocks
    if let Some(global) = section.u32() {
        let _ = section.skip(u64::from(global));
    }
    while records.is_empty() {
        let Some(block) = next_block(&mut section, f.big) else { break };
        if [*b"Lr16", *b"Lr32", *b"Layr"].contains(&block.key) {
            records = layer_info(block.data, f)?;
        }
    }
    Ok(records)
}

/// The layer records in a layer info, bottom first, with where each channel's data is.
fn layer_info<S: Source + ?Sized>(mut c: Cursor<'_, S>, f: &Format) -> Result<Vec<Record>, PsdError> {
    let damaged = PsdError::Damaged;
    // (negative: the flattened picture's alpha is the first layer's transparency)
    let count = usize::from(c.i16().ok_or(damaged("layers"))?.unsigned_abs());
    let mut records = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let (top, left, bottom, right) = (c.i32(), c.i32(), c.i32(), c.i32());
        let (Some(top), Some(left), Some(bottom), Some(right)) = (top, left, bottom, right) else {
            return Err(damaged("layer"));
        };
        let n = c.u16().ok_or(damaged("layer"))?;
        let mut channels = Vec::with_capacity(usize::from(n).min(64));
        for _ in 0..n {
            let id = c.i16().ok_or(damaged("layer"))?;
            let len = c.len(f.big).ok_or(damaged("layer"))?;
            channels.push(Channel { id, at: 0, len });
        }
        let sig = c.array::<4>().ok_or(damaged("layer"))?;
        if sig != *b"8BIM" && sig != *b"8B64" {
            return Err(damaged("layer"));
        }
        let blend = c.array::<4>().ok_or(damaged("layer"))?;
        let opacity = c.u8().ok_or(damaged("layer"))?;
        let clipping = c.u8().ok_or(damaged("layer"))?;
        let flags = c.u8().ok_or(damaged("layer"))?;
        c.u8().ok_or(damaged("layer"))?;
        let extra = u64::from(c.u32().ok_or(damaged("layer"))?);
        let extra = c.sub(extra).ok_or(damaged("layer"))?;
        let rect = Rect { top, left, bottom, right };
        let mut record = Record {
            rect,
            channels,
            blend,
            opacity,
            clipped: clipping == 1,
            hidden: flags & 2 != 0,
            flagged_lock: flags & 1 != 0,
            mask: None,
            name: String::new(),
            section: None,
            protection: 0,
            text: None,
        };
        read_extra(extra, f, &mut record);
        records.push(record);
    }
    // each layer's channels' data follows the records, in their order: a channel running past the
    // end is cut to what of it is there
    let end = c.end;
    let mut at = c.at;
    for ch in records.iter_mut().flat_map(|r| r.channels.iter_mut()) {
        ch.at = at;
        at = at.saturating_add(ch.len);
        ch.len = ch.len.min(end.saturating_sub(ch.at));
    }
    Ok(records)
}

/// A record's extra data: its mask, name, and of the additional information only the name,
/// group marker, locks and text (effects, vector masks, adjustments, fill opacity and the rest
/// are left out).
fn read_extra<S: Source + ?Sized>(mut x: Cursor<'_, S>, f: &Format, record: &mut Record) {
    // the mask's rectangle and settings: a plain mask's, or with a vector mask's after (the first
    // rectangle is then the one the vector mask was drawn into); masks with density or feather
    // settings are left out
    let n = x.u32().map_or(0, u64::from);
    let mask = x.sub(n.min(x.remaining()));
    if let Some(mut m) = mask.filter(|_| n == 20 || n == 36) {
        let (top, left, bottom, right) = (m.i32(), m.i32(), m.i32(), m.i32());
        if let (Some(top), Some(left), Some(bottom), Some(right), Some(outside), Some(flags)) =
            (top, left, bottom, right, m.u8(), m.u8())
        {
            let rect = Rect { top, left, bottom, right };
            record.mask = Some(MaskRect { rect, outside, hidden: flags & 2 != 0 });
        }
    }
    // blending ranges
    let n = x.u32().map_or(0, u64::from);
    let _ = x.skip(n.min(x.remaining()));
    // the name, padded to four with its length byte (what isn't UTF-8 left out)
    let len = x.u8().unwrap_or(0);
    let pascal = x.bytes(u64::from(len)).unwrap_or_default();
    record.name = String::from_utf8_lossy(&pascal).replace('\u{FFFD}', "");
    let _ = x.skip(u64::from((4 - (u32::from(len) + 1) % 4) % 4));
    while let Some(mut b) = next_block(&mut x, f.big) {
        match &b.key {
            b"luni" if b.data.remaining() <= MOST_NAME => {
                let data = b.data.bytes(b.data.remaining()).unwrap_or_default();
                let mut d = Reader::new(&data);
                let n = d.u32().unwrap_or(0) as usize;
                let units: Vec<u16> = (0..n).map_while(|_| d.u16()).take_while(|&u| u != 0).collect();
                let unicode = String::from_utf16_lossy(&units);
                if !unicode.trim().is_empty() {
                    record.name = unicode;
                }
            }
            // a group's marker, or (a group in a group, as some write it) a nested one's
            b"lsct" | b"lsdk" => record.section = b.data.u32(),
            b"lspf" => record.protection = b.data.u32().unwrap_or(0),
            b"TySh" if b.data.remaining() <= MOST_TEXT => {
                record.text = b.data.bytes(b.data.remaining()).and_then(|data| text::layer_text(&data));
            }
            _ => {}
        }
    }
}

/// One block of additional information: its key and data.
struct Block<'s, S: Source + ?Sized> {
    key: [u8; 4],
    data: Cursor<'s, S>,
}

/// The next block of additional information in `c`, if there is one.
fn next_block<'s, S: Source + ?Sized>(c: &mut Cursor<'s, S>, big: bool) -> Option<Block<'s, S>> {
    // its signature, past the padding some writers leave after the block before (to an even
    // length, or to four bytes) and others don't
    let mut tries = 0;
    loop {
        let at = c.at;
        let sig = c.array::<4>()?;
        if sig == *b"8BIM" || sig == *b"8B64" {
            break;
        }
        tries += 1;
        if tries > 3 {
            return None;
        }
        c.at = at + 1;
    }
    let key = c.array::<4>()?;
    // in a PSB these keep eight-byte lengths
    const LONG: [&[u8; 4]; 13] = [
        b"LMsk", b"Lr16", b"Lr32", b"Layr", b"Mt16", b"Mt32", b"Mtrn", b"Alph", b"FMsk", b"lnk2", b"FEid", b"FXid",
        b"PxSD",
    ];
    let n = if big && LONG.contains(&&key) { c.u64()? } else { u64::from(c.u32()?) };
    let data = c.sub(n)?;
    Some(Block { key, data })
}

impl<S: Source + ?Sized> Parsed<'_, S> {
    /// A layer's pixels on the canvas, premultiplied: none if no part of it is there, a colour
    /// channel is missing, or it is all clear.
    fn layer_pixels(&self, r: &Record) -> Option<Vec<u8>> {
        let f = &self.format;
        let win = r.window(f)?;
        let alpha = r.channel(-1).map(|ch| channel_window(self.src, ch, r.rect, win, f, false, 0));
        if alpha.as_ref().is_some_and(|a| a.iter().all(|&v| v == 0)) {
            return None;
        }
        let planes: Vec<Vec<u8>> = (0..f.colour.channels())
            .map(|c| {
                let ch = i16::try_from(c).ok().and_then(|c| r.channel(c));
                ch.map(|ch| channel_window(self.src, ch, r.rect, win, f, true, 0)).unwrap_or_default()
            })
            .collect();
        let (w, h) = f.canvas();
        let mut out = vec![0u8; w * h * 4];
        let ww = win.width();
        for wy in 0..win.height() {
            let at = ((win.y0 + wy) * w + win.x0) * 4;
            let Some(row) = out.get_mut(at..at + ww * 4) else { continue };
            for (wx, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let i = wy * ww + wx;
                let s = |c: usize| planes.get(c).and_then(|p| p.get(i)).copied().unwrap_or(0);
                // no alpha channel: opaque (as a background layer is)
                let a = alpha.as_ref().map_or(255, |p| p.get(i).copied().unwrap_or(0));
                let [r, g, b] = rgb(f, s);
                *px = premultiply([r, g, b, a]);
            }
        }
        Some(out)
    }

    /// A layer's mask on the canvas: its grey in every channel, opaque.
    fn mask_pixels(&self, r: &Record) -> Option<Vec<u8>> {
        let f = &self.format;
        let m = r.mask()?;
        let ch = r.channel(-2)?;
        let (w, h) = f.canvas();
        let mut out: Vec<u8> = std::iter::repeat_n([m.outside, m.outside, m.outside, 255], w * h).flatten().collect();
        if let Some(win) = m.rect.on_canvas(f) {
            let plane = channel_window(self.src, ch, m.rect, win, f, false, m.outside);
            let ww = win.width();
            for wy in 0..win.height() {
                let at = ((win.y0 + wy) * w + win.x0) * 4;
                let Some(row) = out.get_mut(at..at + ww * 4) else { continue };
                for (wx, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let v = plane.get(wy * ww + wx).copied().unwrap_or(m.outside);
                    *px = [v, v, v, 255];
                }
            }
        }
        Some(out)
    }

    /// The colour of a layer that shows as one opaque colour over the whole canvas: a background.
    fn uniform(&self, r: &Record) -> Option<[u8; 3]> {
        let f = &self.format;
        let shown_as_is = !r.hidden && r.opacity == 255 && r.text.is_none() && r.section.is_none_or(|s| s == 0);
        let unmasked = r.mask().is_none_or(|m| m.hidden);
        let win = r.window(f).filter(|w| w.whole(f) && shown_as_is && unmasked)?;
        let alpha = r.channel(-1).map(|ch| channel_window(self.src, ch, r.rect, win, f, false, 0));
        if alpha.as_ref().is_some_and(|a| a.iter().any(|&v| v != 255)) {
            return None;
        }
        let planes: Vec<Vec<u8>> = (0..f.colour.channels())
            .map(|c| {
                let ch = i16::try_from(c).ok().and_then(|c| r.channel(c));
                ch.map(|ch| channel_window(self.src, ch, r.rect, win, f, true, 0)).unwrap_or_default()
            })
            .collect();
        let first: Vec<u8> = planes.iter().map(|p| p.first().copied().unwrap_or(0)).collect();
        if planes.iter().zip(&first).any(|(p, v)| p.iter().any(|s| s != v)) {
            return None;
        }
        Some(rgb(f, |c| first.get(c).copied().unwrap_or(0)))
    }
}

/// The samples of channel `ch` (a plane the size of `rect`, placed as it says on the canvas)
/// within `win`, in 8 bits, a row of the window after another; what is missing or doesn't unpack
/// is `fill`. Only the rows the window needs are unpacked, and only its columns kept.
fn channel_window<S: Source + ?Sized>(
    src: &S,
    ch: &Channel,
    rect: Rect,
    win: Window,
    f: &Format,
    colour: bool,
    fill: u8,
) -> Vec<u8> {
    let (ww, wh) = (win.width(), win.height());
    let mut out = vec![fill; ww * wh];
    let (w, h) = (rect.width(), rect.height());
    // the window in the plane's own columns and rows
    let px0 = usize::try_from(win.x0 as i64 - i64::from(rect.left)).unwrap_or(0);
    let py0 = usize::try_from(win.y0 as i64 - i64::from(rect.top)).unwrap_or(0);
    let Some(row) = row_bytes(w, f.sample) else { return out };
    if px0 + ww > w || py0 + wh > h {
        return out;
    }
    let mut c = Cursor::new(src, ch.at, ch.at.saturating_add(ch.len));
    let Some(compression) = c.u16() else { return out };
    let mut put = |y: usize, raw: &[u8], skip: usize| {
        if let Some(dst) = out.get_mut(y * ww..(y + 1) * ww) {
            samples_to8(raw, skip, f.sample, colour, dst);
        }
    };
    match compression {
        0 => {
            // the bytes of the window's columns in each of its rows
            let (from, to, skip) = if f.sample == 0 {
                (px0 / 8, (px0 + ww).div_ceil(8), px0 % 8)
            } else {
                (px0 * f.sample, (px0 + ww) * f.sample, 0)
            };
            let rows_at = c.at;
            for y in 0..wh {
                let at = rows_at.saturating_add((py0 + y) as u64 * row as u64 + from as u64);
                if c.seek(at).is_none() {
                    break;
                }
                let Some(raw) = c.slice(to - from) else { break };
                put(y, raw, skip);
            }
        }
        1 => {
            let size = if f.big { 4 } else { 2 };
            let counts_at = c.at;
            let data_at = counts_at.saturating_add(h as u64 * size);
            let mut counts = Cursor::new(src, counts_at, data_at.min(c.end));
            let mut data = Cursor::new(src, data_at, c.end);
            let mut line = Vec::with_capacity(row);
            for py in 0..py0 + wh {
                let Some(n) = next_count(&mut counts, f.big) else { break };
                if py < py0 {
                    if data.skip(n).is_none() {
                        break;
                    }
                    continue;
                }
                if !packed_row(&mut data, n, row, &mut line) {
                    break;
                }
                put(py - py0, &line, px0);
            }
        }
        2 | 3 => {
            let mut z = Inflater::new(c, row);
            for py in 0..py0 + wh {
                let Some(line) = z.next_row() else { break };
                if py < py0 {
                    continue;
                }
                if compression == 3 {
                    undo_prediction(line, row, f.sample);
                }
                put(py - py0, line, px0);
            }
        }
        _ => {}
    }
    out
}

/// The number of packed bytes of the next row.
fn next_count<S: Source + ?Sized>(counts: &mut Cursor<'_, S>, big: bool) -> Option<u64> {
    if big { counts.u32().map(u64::from) } else { counts.u16().map(u64::from) }
}

/// A row of `row` bytes unpacked into `line` from the next `n` packed bytes of `data`; false if
/// they aren't there. No more is read than a row can take packed: a count past that is damage,
/// and what is past it is passed over.
fn packed_row<S: Source + ?Sized>(data: &mut Cursor<'_, S>, n: u64, row: usize, line: &mut Vec<u8>) -> bool {
    let most = (row as u64).saturating_mul(2).saturating_add(2);
    let take = n.min(most).min(data.remaining());
    let Some(packed) = usize::try_from(take).ok().and_then(|t| data.slice(t)) else { return false };
    line.clear();
    unpack(packed, row, line);
    let _ = data.skip(n - take);
    true
}

/// PackBits: a row of `row` bytes unpacked onto `out` (short rows filled with zeros).
fn unpack(packed: &[u8], row: usize, out: &mut Vec<u8>) {
    let end = out.len() + row;
    let mut i = packed.iter();
    while out.len() < end {
        let Some(&n) = i.next() else { break };
        let n = n as i8;
        if n >= 0 {
            for _ in 0..=n {
                out.push(i.next().copied().unwrap_or(0));
            }
        } else if n != -128 {
            let v = i.next().copied().unwrap_or(0);
            out.extend(std::iter::repeat_n(v, usize::from((-n).unsigned_abs()) + 1));
        }
    }
    out.resize(end, 0);
}

/// A zlib stream unpacked a row at a time, held no more than a row and a buffer of its input.
struct Inflater<'s, S: Source + ?Sized> {
    data: Cursor<'s, S>,
    state: Box<InflateState>,
    line: Vec<u8>,
    ended: bool,
}

impl<'s, S: Source + ?Sized> Inflater<'s, S> {
    fn new(data: Cursor<'s, S>, row: usize) -> Self {
        Inflater { data, state: InflateState::new_boxed(DataFormat::Zlib), line: vec![0; row], ended: false }
    }

    /// The next row; none once the stream has ended or doesn't unpack.
    fn next_row(&mut self) -> Option<&mut [u8]> {
        let Inflater { data, state, line, ended } = self;
        let mut filled = 0;
        while filled < line.len() {
            if *ended {
                return None;
            }
            let input = data.peek(AHEAD);
            let out = line.get_mut(filled..).unwrap_or_default();
            let r = inflate(state, input, out, MZFlush::None);
            filled += r.bytes_written;
            let consumed = r.bytes_consumed as u64;
            let _ = data.skip(consumed);
            match r.status {
                Ok(MZStatus::StreamEnd) => *ended = true,
                // (wanting more input: it is read next time round)
                Ok(_) | Err(MZError::Buf) => {}
                Err(_) => return None,
            }
            // nothing read and nothing made: the input has run out, or doesn't unpack
            if consumed == 0 && r.bytes_written == 0 && !*ended {
                return None;
            }
        }
        Some(line.as_mut_slice())
    }
}

/// `dst.len()` samples from `raw` (from its `skip`th) in 8 bits: a bitmap's bits black (set) or
/// white, 16-bit ones rounded, 32-bit floats (linear light) a `colour` channel's through the sRGB
/// tone curve, transparency's as they are, both clamped. Those `raw` hasn't are 0.
fn samples_to8(raw: &[u8], skip: usize, sample: usize, colour: bool, dst: &mut [u8]) {
    if sample == 1
        && let Some(part) = raw.get(skip..skip + dst.len())
    {
        dst.copy_from_slice(part);
        return;
    }
    for (i, v) in dst.iter_mut().enumerate() {
        let k = skip + i;
        *v = match sample {
            0 => {
                if raw.get(k / 8).is_some_and(|b| b >> (7 - k % 8) & 1 == 1) {
                    0
                } else {
                    255
                }
            }
            2 => raw
                .get(2 * k..)
                .and_then(|s| s.first_chunk::<2>())
                .map_or(0, |&s| ((u32::from(u16::from_be_bytes(s)) * 255 + 32767) / 65535) as u8),
            4 => raw.get(4 * k..).and_then(|s| s.first_chunk::<4>()).map_or(0, |&s| {
                let v = f32::from_be_bytes(s);
                let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
                let v = if colour { crate::profile::encode(v) } else { v };
                (v * 255.0).round() as u8
            }),
            _ => raw.get(k).copied().unwrap_or(0),
        };
    }
}

/// ZIP with prediction keeps each sample as its difference from the one before it in the row; a
/// 32-bit row keeps each byte so, its floats' bytes laid out as four planes (first bytes, then
/// second...).
fn undo_prediction(data: &mut [u8], row: usize, sample: usize) {
    let mut planar = Vec::new();
    for line in data.chunks_mut(row.max(1)) {
        if sample == 4 {
            let mut last = 0u8;
            for b in line.iter_mut() {
                last = last.wrapping_add(*b);
                *b = last;
            }
            planar.clear();
            planar.extend_from_slice(line);
            let w = line.len() / 4;
            for (x, s) in line.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                for (k, v) in s.iter_mut().enumerate() {
                    *v = planar.get(k * w + x).copied().unwrap_or(0);
                }
            }
        } else if sample == 2 {
            let mut last = 0u16;
            for s in line.as_chunks_mut::<2>().0 {
                last = last.wrapping_add(u16::from_be_bytes(*s));
                *s = last.to_be_bytes();
            }
        } else {
            let mut last = 0u8;
            for b in line.iter_mut() {
                last = last.wrapping_add(*b);
                *b = last;
            }
        }
    }
}

/// The flattened picture at `at` (straight RGBA, the canvas's size), as far as it reads; none if
/// it isn't there at all.
fn merged<S: Source + ?Sized>(src: &S, at: u64, f: &Format, channels: u16) -> Option<Vec<u8>> {
    let (w, h) = f.canvas();
    let mut rgba = vec![0u8; w * h * 4];
    let read = merged_rows(src, at, f, channels, |y, row| {
        if let Some(dst) = rgba.get_mut(y * w * 4..(y + 1) * w * 4) {
            dst.copy_from_slice(row);
        }
    });
    read.then_some(rgba)
}

/// What hands over a channel's next row, unpacked (false once there are none).
type RowReader<'a> = Box<dyn FnMut(&mut Vec<u8>) -> bool + 'a>;

/// The flattened picture at `at`, a row at a time (straight RGBA, top first) to `row`, holding a
/// row of each channel (a stream packed whole, which no writer is known to use for it, is unpacked
/// whole first); false if none of it reads. Rows past what reads come clear.
fn merged_rows<S: Source + ?Sized>(
    src: &S,
    at: u64,
    f: &Format,
    channels: u16,
    mut row: impl FnMut(usize, &[u8]),
) -> bool {
    let (w, h) = f.canvas();
    let colours = f.colour.channels();
    let count = usize::from(channels).max(colours);
    // the colours and transparency (the rest, spot colours and the like, left out)
    let wanted = count.min(colours + 1);
    let Some(bytes) = row_bytes(w, f.sample) else { return false };
    let mut c = Cursor::new(src, at, src.size());
    let Some(compression) = c.u16() else { return false };
    // each channel's next row, unpacked: its rows one after another, the channels one after another
    // (a stream packed whole is unpacked into these first)
    let mut planes: Vec<Vec<u8>> = Vec::new();
    let mut readers: Vec<RowReader<'_>> = Vec::with_capacity(wanted);
    match compression {
        0 => {
            for k in 0..wanted {
                let start = c.at.saturating_add((k * h) as u64 * bytes as u64);
                let mut d = Cursor::new(src, start, c.end);
                readers.push(Box::new(move |line: &mut Vec<u8>| {
                    let Some(raw) = d.slice(bytes) else { return false };
                    line.clear();
                    line.extend_from_slice(raw);
                    true
                }));
            }
        }
        1 => {
            // every row's count first, all the channels', then the rows: where each channel's rows
            // start is the sum of the counts before it
            let size = if f.big { 4 } else { 2 };
            let data_at = c.at.saturating_add((count * h) as u64 * size);
            let mut counts = Cursor::new(src, c.at, data_at.min(c.end));
            let mut start = data_at;
            for _ in 0..wanted {
                let from = counts.at;
                let mut sum = 0u64;
                for _ in 0..h {
                    sum = sum.saturating_add(next_count(&mut counts, f.big).unwrap_or(0));
                }
                let mut own = Cursor::new(src, from, counts.at);
                let mut data = Cursor::new(src, start, c.end);
                start = start.saturating_add(sum);
                readers.push(Box::new(move |line: &mut Vec<u8>| {
                    let Some(n) = next_count(&mut own, f.big) else { return false };
                    packed_row(&mut data, n, bytes, line)
                }));
            }
        }
        2 | 3 => {
            planes.extend((0..wanted).map(|_| Vec::with_capacity(bytes * h)));
            let mut z = Inflater::new(c, bytes);
            'all: for plane in &mut planes {
                for _ in 0..h {
                    let Some(line) = z.next_row() else { break 'all };
                    if compression == 3 {
                        undo_prediction(line, bytes, f.sample);
                    }
                    plane.extend_from_slice(line);
                }
            }
            for plane in &planes {
                let mut rows = plane.chunks(bytes.max(1));
                readers.push(Box::new(move |line: &mut Vec<u8>| {
                    let Some(raw) = rows.next() else { return false };
                    line.clear();
                    line.extend_from_slice(raw);
                    true
                }));
            }
        }
        _ => return false,
    }
    let mut lines: Vec<Vec<u8>> = (0..wanted).map(|_| Vec::with_capacity(bytes)).collect();
    let mut samples: Vec<Vec<u8>> = (0..wanted).map(|_| vec![0u8; w]).collect();
    let mut rgba = vec![0u8; w * 4];
    let mut any = false;
    for y in 0..h {
        for (k, ((next, line), out)) in readers.iter_mut().zip(&mut lines).zip(&mut samples).enumerate() {
            if next(line) {
                samples_to8(line, 0, f.sample, k < colours, out);
                any |= k == 0;
            } else {
                out.fill(0);
            }
        }
        let alpha = samples.get(colours);
        for (x, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let s = |c: usize| samples.get(c).and_then(|p| p.get(x)).copied().unwrap_or(0);
            let [r, g, b] = rgb(f, s);
            *px = [r, g, b, alpha.and_then(|a| a.get(x)).copied().unwrap_or(255)];
        }
        row(y, &rgba);
    }
    any
}

/// The flattened picture at `at` on white, made small to fit `side` (each pixel the average of
/// those it covers), read a row at a time: its size and opaque RGBA; none if it isn't there.
fn merged_thumbnail<S: Source + ?Sized>(
    src: &S,
    at: u64,
    f: &Format,
    channels: u16,
    side: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h) = (f.width, f.height);
    let scale = (side as f32 / w.max(h).max(1) as f32).min(1.0);
    let (tw, th) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    let (tw_, th_) = (tw as usize, th as usize);
    // each target pixel's sums of red, green and blue on white, and how many it covers
    let mut sums = vec![[0u32; 4]; tw_ * th_];
    // the target column each column falls in
    let columns: Vec<usize> = (0..w as usize).map(|x| (x * tw_ / w.max(1) as usize).min(tw_ - 1)).collect();
    let read = merged_rows(src, at, f, channels, |y, row| {
        let ty = (y * th_ / h.max(1) as usize).min(th_ - 1);
        let Some(target) = sums.get_mut(ty * tw_..(ty + 1) * tw_) else { return };
        for (&[r, g, b, a], &tx) in row.as_chunks::<4>().0.iter().zip(&columns) {
            let over = |c: u8| (u32::from(c) * u32::from(a) + 255 * (255 - u32::from(a)) + 127) / 255;
            if let Some(t) = target.get_mut(tx) {
                let [sr, sg, sb, n] = *t;
                *t = [sr + over(r), sg + over(g), sb + over(b), n + 1];
            }
        }
    });
    if !read {
        return None;
    }
    let rgba = sums
        .iter()
        .flat_map(|&[r, g, b, n]| {
            let n = n.max(1);
            [r, g, b].map(|v| ((v + n / 2) / n) as u8).into_iter().chain([255])
        })
        .collect();
    Some((tw, th, rgba))
}

/// A pixel's colour from its samples (CMYK's kept inverted, as PSDs keep them).
fn rgb(f: &Format, s: impl Fn(usize) -> u8) -> [u8; 3] {
    match f.colour {
        Colour::Bitmap | Colour::Grey => [s(0); 3],
        Colour::Indexed => f.palette.get(usize::from(s(0))).copied().unwrap_or([0; 3]),
        Colour::Lab => lab_to_srgb(s(0), s(1), s(2)),
        Colour::Rgb => [s(0), s(1), s(2)],
        Colour::Cmyk => {
            let k = u16::from(s(3));
            [0, 1, 2].map(|c| ((u16::from(s(c)) * k + 127) / 255) as u8)
        }
    }
}

/// A L*a*b* colour (D50) as a PSD keeps it in 8 bits (L 0..255 for 0..100, a and b 128 for
/// none) brought to sRGB (the white point adapted to D65), clipped.
fn lab_to_srgb(l: u8, a: u8, b: u8) -> [u8; 3] {
    let l = f32::from(l) * 100.0 / 255.0;
    let (a, b) = (f32::from(a) - 128.0, f32::from(b) - 128.0);
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 500.0, fy - b / 200.0);
    let back = |t: f32| if t > 6.0 / 29.0 { t * t * t } else { 3.0 * (6.0f32 / 29.0).powi(2) * (t - 4.0 / 29.0) };
    let (x, y, z) = (0.9642 * back(fx), back(fy), 0.8249 * back(fz));
    // D50 XYZ to linear sRGB (Bradford adapted)
    let linear = [
        3.133_856 * x - 1.616_867 * y - 0.490_615 * z,
        -0.978_768 * x + 1.916_142 * y + 0.033_454 * z,
        0.071_945 * x - 0.228_991 * y + 1.405_243 * z,
    ];
    linear.map(|c| (crate::profile::encode(c.clamp(0.0, 1.0)) * 255.0).round() as u8)
}

/// A straight RGBA pixel premultiplied.
fn premultiply([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    let m = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
    [m(r), m(g), m(b), a]
}

/// Straight RGBA made premultiplied.
fn premultiplied(straight: &[u8]) -> Vec<u8> {
    straight.as_chunks::<4>().0.iter().flat_map(|&px| premultiply(px)).collect()
}

fn paint_layer(id: LayerId, name: String, parent: Option<LayerId>) -> Layer {
    Layer {
        id,
        name,
        visible: true,
        opacity: 1.0,
        blend: BlendMode::Normal,
        alpha_lock: false,
        clipped: false,
        locked: false,
        reference: false,
        kind: LayerKind::Paint,
        parent,
        collapsed: false,
        text: None,
        private: false,
        assisted: false,
        control: None,
        plugin: None,
        filter: None,
        marks: Default::default(),
    }
}

/// The document's layers (bottom first), where each one's pixels come from, and the top layer.
struct Plan {
    layers: Vec<Layer>,
    pixels: Vec<(LayerId, Pixels)>,
    top: Option<LayerId>,
}

/// The document's layers from the records; the bottom one left out if it is the `background`.
fn plan(records: &[Record], f: &Format, background: bool) -> Plan {
    let mut layers = Vec::new();
    let mut pixels = Vec::new();
    let mut next = 1u64;
    let mut new_id = || {
        next += 1;
        LayerId(next - 1)
    };
    // the groups open around the records being read
    let mut open: Vec<LayerId> = Vec::new();
    let mut top = None;
    for (i, r) in records.iter().enumerate() {
        // a layer without a name is numbered by where it is, counting from 2 at the bottom
        let name = if r.name.is_empty() { format!("Layer {}", i + 2) } else { r.name.clone() };
        match r.section {
            Some(3) => {
                open.push(new_id());
                continue;
            }
            // a group keeps its name, whether it is open, hidden or locked; not its blend mode,
            // opacity or clipping
            Some(1 | 2) => {
                let id = open.pop().unwrap_or_else(&mut new_id);
                layers.push(Layer {
                    kind: LayerKind::Group,
                    visible: !r.hidden,
                    locked: r.locked(),
                    collapsed: r.section == Some(2),
                    ..paint_layer(id, name, open.last().copied())
                });
                continue;
            }
            _ => {}
        }
        // the bottom layer all of one colour: the background
        if i == 0 && background {
            continue;
        }
        // nothing on the canvas and no text: left out, its mask too
        let on_canvas = r.window(f).is_some();
        if !on_canvas && r.text.is_none() {
            continue;
        }
        let id = new_id();
        layers.push(Layer {
            visible: !r.hidden,
            opacity: f32::from(r.opacity) / 255.0,
            blend: blend_mode(&r.blend),
            clipped: r.clipped,
            locked: r.locked(),
            alpha_lock: r.alpha_locked(),
            text: r.text.clone(),
            ..paint_layer(id, name, open.last().copied())
        });
        if on_canvas {
            pixels.push((id, Pixels::Record(i)));
        }
        top = Some(id);
        // its mask, just above it (hidden if turned off)
        if let Some(m) = r.mask() {
            let mid = new_id();
            layers.push(Layer {
                kind: LayerKind::Mask,
                name: "Mask".to_owned(),
                visible: !m.hidden,
                ..paint_layer(mid, String::new(), Some(id))
            });
            pixels.push((mid, Pixels::Mask(i)));
        }
    }
    Plan { layers, pixels, top }
}

/// A blend mode from its key (pass-through groups, dissolve and anything unknown: normal).
fn blend_mode(key: &[u8; 4]) -> BlendMode {
    match key {
        b"mul " => BlendMode::Multiply,
        b"dark" => BlendMode::Darken,
        b"idiv" => BlendMode::ColorBurn,
        b"lbrn" => BlendMode::LinearBurn,
        b"dkCl" => BlendMode::DarkerColor,
        b"lite" => BlendMode::Lighten,
        b"scrn" => BlendMode::Screen,
        b"div " => BlendMode::ColorDodge,
        b"lddg" => BlendMode::Add,
        b"lgCl" => BlendMode::LighterColor,
        b"over" => BlendMode::Overlay,
        b"sLit" => BlendMode::SoftLight,
        b"hLit" => BlendMode::HardLight,
        b"vLit" => BlendMode::VividLight,
        b"lLit" => BlendMode::LinearLight,
        b"pLit" => BlendMode::PinLight,
        b"hMix" => BlendMode::HardMix,
        b"diff" => BlendMode::Difference,
        b"smud" => BlendMode::Exclusion,
        b"fsub" => BlendMode::Subtract,
        b"fdiv" => BlendMode::Divide,
        b"hue " => BlendMode::Hue,
        b"sat " => BlendMode::Saturation,
        b"colr" => BlendMode::Color,
        b"lum " => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

/// How much a cursor reads ahead at once.
const AHEAD: usize = 64 << 10;

/// Reads big-endian numbers and runs of bytes in order from part of a source (`at` up to `end`),
/// a buffer's worth read ahead at a time; None past its end.
struct Cursor<'s, S: Source + ?Sized> {
    src: &'s S,
    at: u64,
    end: u64,
    /// Bytes read ahead, from `ahead_at`.
    ahead: Vec<u8>,
    ahead_at: u64,
}

impl<'s, S: Source + ?Sized> Cursor<'s, S> {
    fn new(src: &'s S, at: u64, end: u64) -> Self {
        let end = end.min(src.size());
        Cursor { src, at: at.min(end), end, ahead: Vec::new(), ahead_at: 0 }
    }

    fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.at)
    }

    fn skip(&mut self, n: u64) -> Option<()> {
        (n <= self.remaining()).then(|| self.at += n)
    }

    /// Goes to `at` (up to the end).
    fn seek(&mut self, at: u64) -> Option<()> {
        (at <= self.end).then(|| self.at = at)
    }

    /// A cursor over the next `n` bytes, passed over here.
    fn sub(&mut self, n: u64) -> Option<Cursor<'s, S>> {
        let at = self.at;
        self.skip(n)?;
        Some(Cursor::new(self.src, at, at + n))
    }

    /// Whether the next `n` bytes are read ahead (reading them, and a buffer's worth after, if
    /// they aren't); false if they aren't there.
    fn hold(&mut self, n: usize) -> bool {
        let from = self.at.checked_sub(self.ahead_at).and_then(|o| usize::try_from(o).ok());
        if from.and_then(|f| f.checked_add(n)).is_some_and(|e| e <= self.ahead.len()) {
            return true;
        }
        let len = usize::try_from(self.remaining()).unwrap_or(usize::MAX).min(n.max(AHEAD));
        if len < n {
            return false;
        }
        self.ahead.resize(len, 0);
        if !self.src.read_at(self.at, &mut self.ahead) {
            self.ahead.clear();
            return false;
        }
        self.ahead_at = self.at;
        true
    }

    /// The next `n` bytes.
    fn slice(&mut self, n: usize) -> Option<&[u8]> {
        if !self.hold(n) {
            return None;
        }
        let from = usize::try_from(self.at.checked_sub(self.ahead_at)?).ok()?;
        self.at += n as u64;
        self.ahead.get(from..from.checked_add(n)?)
    }

    /// Up to `most` of the next bytes, without passing over them (none at the end).
    fn peek(&mut self, most: usize) -> &[u8] {
        let n = usize::try_from(self.remaining()).unwrap_or(usize::MAX).min(most);
        if n == 0 || !self.hold(n) {
            return &[];
        }
        let from = self.at.checked_sub(self.ahead_at).and_then(|o| usize::try_from(o).ok()).unwrap_or(0);
        self.ahead.get(from..from + n).unwrap_or_default()
    }

    /// The next `n` bytes, as they are (for runs a caller has kept small).
    fn bytes(&mut self, n: u64) -> Option<Vec<u8>> {
        if n > self.remaining() {
            return None;
        }
        let mut out = vec![0; usize::try_from(n).ok()?];
        if !self.src.read_at(self.at, &mut out) {
            return None;
        }
        self.at += n;
        Some(out)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.slice(N)?.first_chunk::<N>().copied()
    }

    fn u8(&mut self) -> Option<u8> {
        self.array::<1>().map(|[b]| b)
    }

    fn u16(&mut self) -> Option<u16> {
        self.array().map(u16::from_be_bytes)
    }

    fn i16(&mut self) -> Option<i16> {
        self.array().map(i16::from_be_bytes)
    }

    fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_be_bytes)
    }

    fn i32(&mut self) -> Option<i32> {
        self.array().map(i32::from_be_bytes)
    }

    fn u64(&mut self) -> Option<u64> {
        self.array().map(u64::from_be_bytes)
    }

    /// A length: four bytes, or eight in a PSB.
    fn len(&mut self, big: bool) -> Option<u64> {
        if big { self.u64() } else { self.u32().map(u64::from) }
    }
}

/// Reads big-endian numbers and runs of bytes in order from bytes in memory; None past the end.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.bytes.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn u8(&mut self) -> Option<u8> {
        self.array::<1>().map(|[b]| b)
    }

    fn u16(&mut self) -> Option<u16> {
        self.array().map(u16::from_be_bytes)
    }

    fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_be_bytes)
    }

    fn i32(&mut self) -> Option<i32> {
        self.array().map(i32::from_be_bytes)
    }

    fn f64(&mut self) -> Option<f64> {
        self.array().map(u64::from_be_bytes).map(f64::from_bits)
    }
}
