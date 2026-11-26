use super::*;
use crate::formats::{PsdKind, PsdLayer, psd};

/// A `w` x `h` picture of one straight RGBA colour.
fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
    std::iter::repeat_n(px, (w * h) as usize).flatten().collect()
}

#[test]
fn layers_groups_and_masks_come_back_as_they_were_written() {
    let (w, h) = (4, 3);
    let mut blue = solid(w, h, [0, 0, 255, 255]);
    blue[4..8].copy_from_slice(&[0, 255, 255, 255]);
    // red on the left half only, half covered
    let mut red = solid(w, h, [0, 0, 0, 0]);
    for y in 0..h as usize {
        for x in 0..2 {
            red[(y * w as usize + x) * 4..][..4].copy_from_slice(&[255, 0, 0, 128]);
        }
    }
    let mask: Vec<u8> = (0..w * h).map(|i| if i % 2 == 0 { 255 } else { 0 }).collect();
    let layers = [
        PsdLayer {
            name: "Group",
            kind: PsdKind::GroupStart { open: false },
            opacity: 1.0,
            visible: true,
            blend: BlendMode::Normal,
            clipped: false,
        },
        PsdLayer {
            name: "Red",
            kind: PsdKind::Pixels { rgba: &red, mask: Some(&mask) },
            opacity: 1.0,
            visible: true,
            blend: BlendMode::Screen,
            clipped: false,
        },
        PsdLayer {
            name: "",
            kind: PsdKind::GroupEnd,
            opacity: 1.0,
            visible: true,
            blend: BlendMode::Normal,
            clipped: false,
        },
        PsdLayer {
            name: "Bläu",
            kind: PsdKind::Pixels { rgba: &blue, mask: None },
            opacity: 0.5,
            visible: false,
            blend: BlendMode::Multiply,
            clipped: false,
        },
    ];
    let bytes = psd(w, h, 300.0, &[], &layers, &blue, None);
    let got = read(&bytes).unwrap();
    let doc = &got.document;
    assert_eq!((doc.width(), doc.height()), (w, h));
    assert!((doc.dpi - 300.0).abs() < 0.01);
    let names: Vec<(&str, LayerKind)> = doc.layers().iter().map(|l| (l.name.as_str(), l.kind)).collect();
    assert_eq!(
        names,
        [("Bläu", LayerKind::Paint), ("Red", LayerKind::Paint), ("Mask", LayerKind::Mask), ("Group", LayerKind::Group)]
    );
    let [bottom, red_layer, mask_layer, group] = doc.layers() else { panic!() };
    // the background hidden: no bottom layer of one colour
    assert!(!doc.background_visible);
    assert!(!bottom.visible);
    assert_eq!(bottom.blend, BlendMode::Multiply);
    assert!((bottom.opacity - 0.5).abs() < 0.01);
    assert_eq!(red_layer.blend, BlendMode::Screen);
    assert_eq!(red_layer.parent, Some(group.id));
    assert!(mask_layer.visible);
    assert_eq!(mask_layer.parent, Some(red_layer.id));
    assert!(group.collapsed);
    // pixels premultiplied, placed where they were
    let px = |id: LayerId| got.pixels.iter().find(|(i, _)| *i == id).map(|(_, p)| p.clone()).unwrap();
    let reds = px(red_layer.id);
    assert_eq!(&reds[0..4], &[128, 0, 0, 128]);
    assert_eq!(&reds[8..12], &[0, 0, 0, 0]);
    assert_eq!(&px(bottom.id)[0..4], &[0, 0, 255, 255]);
    let m = px(mask_layer.id);
    assert_eq!(&m[0..8], &[255, 255, 255, 255, 0, 0, 0, 255]);
    // the flattened picture is there too
    let (fw, fh, flat) = flattened(&bytes).unwrap();
    assert_eq!((fw, fh), (w, h));
    assert_eq!(&flat[0..4], &[0, 0, 255, 255]);
}

#[test]
fn a_file_without_layers_is_one_layer_and_bad_files_are_turned_away() {
    let (w, h) = (3, 2);
    let flat = solid(w, h, [10, 20, 30, 255]);
    let mut bytes = psd(w, h, 72.0, &[], &[], &flat, None);
    let got = read(&bytes).unwrap();
    assert_eq!(got.document.layers().len(), 1);
    assert_eq!(&got.pixels[0].1[0..4], &[10, 20, 30, 255]);
    assert_eq!(read(b"PNG.").err(), Some(PsdError::NotAPsd));
    // multichannel
    bytes[25] = 7;
    assert_eq!(read(&bytes).err(), Some(PsdError::Unsupported("colour mode")));
    bytes[25] = 3;
    bytes.truncate(40);
    assert!(read(&bytes).is_err());
}

/// A layer record for a PSD made by hand: its rectangle (top, left, bottom, right), channels
/// (id and 8-bit samples, packed as the file is), mask data and additional information.
struct Raw<'a> {
    rect: [i32; 4],
    channels: Vec<(i16, Vec<u8>)>,
    blend: &'a [u8; 4],
    opacity: u8,
    clipping: u8,
    /// Bit 0: transparency locked; bit 1: hidden.
    flags: u8,
    mask: Vec<u8>,
    name: &'a str,
    /// The name's bytes as written, in place of `name`'s.
    pascal: Option<Vec<u8>>,
    blocks: Vec<(&'a [u8; 4], Vec<u8>)>,
    /// Each block's data padded to this many bytes (not counted in its length), or not at all.
    pad: usize,
    /// Its channels packed so, whatever the file's are.
    pack: Option<Pack>,
}

impl Default for Raw<'_> {
    fn default() -> Self {
        Raw {
            rect: [0; 4],
            channels: Vec::new(),
            blend: b"norm",
            opacity: 255,
            clipping: 0,
            flags: 0,
            mask: Vec::new(),
            name: "Layer",
            pascal: None,
            blocks: Vec::new(),
            pad: 0,
            pack: None,
        }
    }
}

impl Raw<'_> {
    /// RGBA samples of one colour over its rectangle.
    fn filled(mut self, px: [u8; 4]) -> Self {
        let [top, left, bottom, right] = self.rect;
        let n = ((bottom - top) * (right - left)).max(0) as usize;
        self.channels = [(-1, 3), (0, 0), (1, 1), (2, 2)].map(|(id, c)| (id, vec![px[c]; n])).to_vec();
        self
    }

    /// The width of channel `id`'s rows: its mask's, or its own.
    fn row(&self, id: i16) -> usize {
        let [_, left, _, right] = if id == -2 && self.mask.len() >= 16 {
            let v = |i: usize| i32::from_be_bytes(self.mask[i * 4..i * 4 + 4].try_into().unwrap());
            [v(0), v(1), v(2), v(3)]
        } else {
            self.rect
        };
        (right - left).max(0) as usize
    }
}

fn u32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

/// Mask data: a rectangle, the colour outside it and flags, `len` bytes long.
fn mask_data([top, left, bottom, right]: [i32; 4], outside: u8, flags: u8, len: usize) -> Vec<u8> {
    let mut m: Vec<u8> = [top, left, bottom, right].iter().flat_map(|v| v.to_be_bytes()).collect();
    m.extend([outside, flags]);
    m.resize(len, 0);
    m
}

/// How a hand-made file packs its channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pack {
    Raw,
    Rle,
    Zip,
    ZipPredicted,
    /// A compression no writer uses.
    Unknown,
}

/// How a hand-made file is written.
#[derive(Clone, Copy, Debug)]
struct Make {
    pack: Pack,
    /// 8 or 16 bits a channel.
    depth: u16,
    /// A PSB.
    big: bool,
    /// Its layer count written negative.
    negative: bool,
}

impl Default for Make {
    fn default() -> Self {
        Make { pack: Pack::Raw, depth: 8, big: false, negative: false }
    }
}

/// Rows `row` samples long, packed as `pack` (8-bit `samples`, widened to the `make`'s depth),
/// their compression first.
fn packed(samples: &[u8], row: usize, pack: Pack, make: &Make) -> Vec<u8> {
    let wide: Vec<u8> = if make.depth == 16 {
        samples.iter().flat_map(|&v| (u16::from(v) * 257).to_be_bytes()).collect()
    } else {
        samples.to_vec()
    };
    let bytes_row = row * usize::from(make.depth / 8);
    let rows: Vec<&[u8]> = if bytes_row == 0 { Vec::new() } else { wide.chunks(bytes_row).collect() };
    let mut out = Vec::new();
    match pack {
        Pack::Raw => {
            out.extend(0u16.to_be_bytes());
            out.extend(&wide);
        }
        Pack::Unknown => {
            out.extend(7u16.to_be_bytes());
            out.extend(&wide);
        }
        Pack::Rle => {
            out.extend(1u16.to_be_bytes());
            let rows: Vec<Vec<u8>> = rows.iter().map(|r| packbits(r)).collect();
            for r in &rows {
                if make.big {
                    out.extend((r.len() as u32).to_be_bytes());
                } else {
                    out.extend((r.len() as u16).to_be_bytes());
                }
            }
            out.extend(rows.concat());
        }
        Pack::Zip => {
            out.extend(2u16.to_be_bytes());
            out.extend(miniz_oxide::deflate::compress_to_vec_zlib(&wide, 6));
        }
        Pack::ZipPredicted => {
            out.extend(3u16.to_be_bytes());
            let mut deltas = Vec::with_capacity(wide.len());
            for r in &rows {
                if make.depth == 16 {
                    let mut last = 0u16;
                    for s in r.chunks(2) {
                        let v = u16::from_be_bytes([s[0], s[1]]);
                        deltas.extend(v.wrapping_sub(last).to_be_bytes());
                        last = v;
                    }
                } else {
                    let mut last = 0u8;
                    for &v in *r {
                        deltas.push(v.wrapping_sub(last));
                        last = v;
                    }
                }
            }
            out.extend(miniz_oxide::deflate::compress_to_vec_zlib(&deltas, 6));
        }
    }
    out
}

/// PackBits: runs of three or more alike as repeats, the rest as literals.
fn packbits(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < row.len() {
        let v = row[i];
        let mut run = 1;
        while i + run < row.len() && row[i + run] == v && run < 128 {
            run += 1;
        }
        if run >= 3 {
            out.push((1 - run as i32) as i8 as u8);
            out.push(v);
            i += run;
        } else {
            let start = i;
            while i < row.len() && i - start < 128 {
                if i + 2 < row.len() && row[i] == row[i + 1] && row[i] == row[i + 2] {
                    break;
                }
                i += 1;
            }
            out.push((i - start - 1) as u8);
            out.extend(&row[start..i]);
        }
    }
    out
}

/// An RGB PSD `w` x `h` of `records` (bottom first) and a flattened picture of one colour,
/// written as `make` says.
fn make_psd(w: u32, h: u32, records: &[Raw<'_>], flat: [u8; 4], make: &Make) -> Vec<u8> {
    let len = |out: &mut Vec<u8>, n: usize| {
        if make.big {
            out.extend((n as u64).to_be_bytes());
        } else {
            out.extend((n as u32).to_be_bytes());
        }
    };
    let mut out = b"8BPS".to_vec();
    out.extend(if make.big { 2u16 } else { 1 }.to_be_bytes());
    out.extend([0; 6]);
    out.extend(4u16.to_be_bytes());
    out.extend(u32s(&[h, w]));
    out.extend(make.depth.to_be_bytes());
    out.extend(3u16.to_be_bytes());
    out.extend(u32s(&[0, 0]));
    let count = records.len() as i16;
    let mut info = if make.negative { -count } else { count }.to_be_bytes().to_vec();
    let mut data = Vec::new();
    for r in records {
        info.extend(r.rect.iter().flat_map(|v| v.to_be_bytes()));
        info.extend((r.channels.len() as u16).to_be_bytes());
        for (id, samples) in &r.channels {
            let bytes = packed(samples, r.row(*id), r.pack.unwrap_or(make.pack), make);
            info.extend(id.to_be_bytes());
            len(&mut info, bytes.len());
            data.extend(bytes);
        }
        info.extend(b"8BIM");
        info.extend(r.blend);
        info.extend([r.opacity, r.clipping, r.flags, 0]);
        let mut extra = (r.mask.len() as u32).to_be_bytes().to_vec();
        extra.extend(&r.mask);
        extra.extend(0u32.to_be_bytes());
        let name = r.pascal.clone().unwrap_or_else(|| r.name.as_bytes().to_vec());
        extra.push(name.len() as u8);
        extra.extend(&name);
        while !extra.len().is_multiple_of(4) {
            extra.push(0);
        }
        for (key, block) in &r.blocks {
            extra.extend(b"8BIM");
            extra.extend(*key);
            extra.extend((block.len() as u32).to_be_bytes());
            extra.extend(block);
            if r.pad > 0 {
                while !extra.len().is_multiple_of(r.pad) {
                    extra.push(0);
                }
            }
        }
        info.extend((extra.len() as u32).to_be_bytes());
        info.extend(extra);
    }
    info.extend(data);
    let mut section = Vec::new();
    len(&mut section, info.len());
    section.extend(info);
    section.extend(0u32.to_be_bytes());
    len(&mut out, section.len());
    out.extend(section);
    // the flattened picture, raw: four planes
    out.extend(0u16.to_be_bytes());
    for c in flat {
        for _ in 0..w * h {
            if make.depth == 16 {
                out.extend((u16::from(c) * 257).to_be_bytes());
            } else {
                out.push(c);
            }
        }
    }
    out
}

/// An RGB PSD `w` x `h` of `records` (bottom first, unpacked) and a flattened picture of one
/// colour.
fn raw_psd(w: u32, h: u32, records: &[Raw<'_>], flat: [u8; 4]) -> Vec<u8> {
    make_psd(w, h, records, flat, &Make::default())
}

#[test]
fn effects_vector_masks_adjustments_and_fill_are_left_out_and_empty_layers_dropped() {
    let (w, h) = (6, 4);
    let mut bottom = Raw { rect: [0, 0, 4, 6], name: "Paper", ..Raw::default() }.filled([200, 10, 10, 255]);
    // not all of one colour
    bottom.channels[1].1[0] = 0;
    let records = [
        bottom,
        // its own pixels, at its full opacity; the effects, fill and vector mask unread
        Raw {
            rect: [1, 1, 3, 4],
            opacity: 128,
            name: "Shape",
            blocks: vec![
                (b"iOpa", vec![0, 0, 0, 0]),
                (b"lrFX", vec![0, 0, 0, 0]),
                (b"lfx2", vec![0; 8]),
                (b"vmsk", vec![0; 8]),
                (b"SoCo", vec![0; 8]),
            ],
            ..Raw::default()
        }
        .filled([0, 0, 255, 255]),
        // an adjustment has no pixels: left out with its mask
        Raw {
            mask: mask_data([0, 0, 0, 0], 255, 0, 20),
            channels: vec![(-2, vec![])],
            name: "Curves 1",
            blocks: vec![(b"curv", vec![0; 4])],
            ..Raw::default()
        },
        // so is an empty layer, and one off the canvas
        Raw { name: "Empty", ..Raw::default() },
        Raw { rect: [10, 10, 12, 12], name: "Away", ..Raw::default() }.filled([1, 2, 3, 255]),
        // a fill layer keeps the pixels drawn for it
        Raw { rect: [0, 0, 4, 6], name: "Fill", blocks: vec![(b"SoCo", vec![0; 8])], ..Raw::default() }
            .filled([0, 255, 0, 255]),
    ];
    let got = read(&raw_psd(w, h, &records, [9, 9, 9, 255])).unwrap();
    let names: Vec<&str> = got.document.layers().iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Paper", "Shape", "Fill"]);
    let shape = &got.document.layers()[1];
    assert!((shape.opacity - 128.0 / 255.0).abs() < 1e-6);
    let px = &got.pixels[1].1;
    assert_eq!(&px[(6 + 1) * 4..][..4], &[0, 0, 255, 255]);
    assert_eq!(&px[0..4], &[0, 0, 0, 0]);
    // nothing on the canvas at all: the flattened picture
    let got = read(&raw_psd(w, h, &records[2..5], [9, 9, 9, 255])).unwrap();
    assert_eq!(got.document.layers().len(), 1);
    assert_eq!(&got.pixels[0].1[0..4], &[9, 9, 9, 255]);
}

#[test]
fn a_bottom_layer_of_one_colour_becomes_the_background() {
    let (w, h) = (4, 4);
    let records = [
        // the whole canvas, all of one opaque colour
        Raw { rect: [0, 0, 4, 4], name: "Background", ..Raw::default() }.filled([10, 20, 30, 255]),
        Raw { rect: [0, 0, 1, 1], name: "Dot", ..Raw::default() }.filled([255, 0, 0, 255]),
    ];
    let got = read(&raw_psd(w, h, &records, [0; 4])).unwrap();
    let doc = &got.document;
    assert!(doc.background_visible);
    assert_eq!(doc.background, [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0]);
    let names: Vec<&str> = doc.layers().iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Dot"]);
    // above a bottom layer of two colours it stays a layer
    let mut two = Raw { rect: [0, 0, 1, 2], name: "Two", ..Raw::default() }.filled([255, 0, 0, 255]);
    two.channels[1].1[1] = 0;
    let [solid, _] = records;
    let got = read(&raw_psd(w, h, &[two, solid], [0; 4])).unwrap();
    assert!(!got.document.background_visible);
    assert_eq!(got.document.background, [1.0; 3]);
    assert_eq!(got.document.layers().len(), 2);
    // a layer of one colour that doesn't show as one over the whole canvas stays a layer, and the
    // background stays hidden: on part of it, clear (as an empty bottom layer is), hidden, or
    // see-through
    let dot = || Raw { rect: [0, 0, 1, 1], name: "Dot", ..Raw::default() }.filled([255, 0, 0, 255]);
    let bottoms = [
        Raw { rect: [1, 1, 3, 3], name: "Part", ..Raw::default() }.filled([10, 20, 30, 255]),
        Raw { rect: [0, 0, 4, 4], name: "Clear", ..Raw::default() }.filled([0, 0, 0, 0]),
        Raw { rect: [0, 0, 4, 4], name: "Hidden", flags: 2, ..Raw::default() }.filled([255; 4]),
        Raw { rect: [0, 0, 4, 4], name: "Faint", opacity: 100, ..Raw::default() }.filled([255; 4]),
    ];
    for bottom in bottoms {
        let name = bottom.name;
        let got = read(&raw_psd(w, h, &[bottom, dot()], [0; 4])).unwrap();
        let doc = &got.document;
        assert!(!doc.background_visible, "{name}");
        let names: Vec<&str> = doc.layers().iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, [name, "Dot"]);
        // the clear one comes in clear (not black)
        if name == "Clear" {
            assert!(got.pixels[0].1.iter().all(|&v| v == 0));
        }
    }
}

#[test]
fn locks_groups_masks_and_names_come_in_as_the_file_has_them() {
    let (w, h) = (4, 4);
    let mut base = Raw { rect: [0, 0, 4, 4], name: "Base", ..Raw::default() }.filled([0, 0, 0, 255]);
    base.channels[1].1[3] = 9;
    // transparency, position and everything locked
    base.blocks = vec![(b"lspf", u32s(&[1]))];
    let lsct = |kind: u32| (b"lsct", u32s(&[kind]));
    let records = [
        base,
        Raw { name: "</Layer group>", blocks: vec![lsct(3)], ..Raw::default() },
        Raw {
            rect: [0, 0, 2, 2],
            name: "",
            blocks: vec![(b"lspf", u32s(&[4]))],
            // turned off: kept, hidden
            mask: mask_data([0, 0, 2, 2], 0, 2, 20),
            ..Raw::default()
        }
        .filled([5, 5, 5, 255]),
        Raw {
            rect: [2, 2, 4, 4],
            name: "Feathered",
            blocks: vec![(b"lspf", u32s(&[0x8000_0000]))],
            // density or feather settings: the mask left out
            mask: mask_data([2, 2, 4, 4], 0, 0x10, 28),
            ..Raw::default()
        }
        .filled([6, 6, 6, 255]),
        // its blend mode and opacity are left out
        Raw {
            blend: b"mul ",
            opacity: 51,
            flags: 2,
            name: "Folder",
            blocks: vec![lsct(2), (b"lspf", u32s(&[2]))],
            ..Raw::default()
        },
    ];
    let mut records = records;
    records[2].channels.push((-2, vec![255, 0, 0, 255]));
    records[3].channels.push((-2, vec![255; 4]));
    let got = read(&raw_psd(w, h, &records, [0; 4])).unwrap();
    let doc = &got.document;
    let names: Vec<(&str, LayerKind)> = doc.layers().iter().map(|l| (l.name.as_str(), l.kind)).collect();
    assert_eq!(
        names,
        [
            ("Base", LayerKind::Paint),
            ("Layer 4", LayerKind::Paint),
            ("Mask", LayerKind::Mask),
            ("Feathered", LayerKind::Paint),
            ("Folder", LayerKind::Group)
        ]
    );
    let [base, unnamed, mask, feathered, folder] = doc.layers() else { panic!() };
    assert!(base.alpha_lock && !base.locked);
    assert!(unnamed.locked && !unnamed.alpha_lock);
    assert!(!mask.visible);
    assert_eq!(&got.pixels[2].1[0..8], &[255, 255, 255, 255, 0, 0, 0, 255]);
    assert!(feathered.locked && feathered.alpha_lock);
    assert!(folder.collapsed && !folder.visible && folder.locked);
    assert_eq!((folder.blend, folder.opacity), (BlendMode::Normal, 1.0));
    assert_eq!((unnamed.parent, feathered.parent), (Some(folder.id), Some(folder.id)));
}

#[test]
fn a_text_layer_keeps_its_text_and_pixels() {
    let (w, h) = (8, 4);
    let tool = text::tests::type_tool(
        "Hello",
        "Helvetica-Bold",
        10.0,
        [0.0, 0.0, 1.0],
        0,
        [1.0, 0.0, 0.0, 1.0, 2.0, 3.0],
        [0.0, -3.0, 5.0, 1.0],
        false,
    );
    let mut paper = Raw { rect: [0, 0, 4, 8], name: "Paper", ..Raw::default() }.filled([255; 4]);
    paper.channels[1].1[0] = 0;
    let records = [
        paper,
        Raw { rect: [0, 2, 3, 6], name: "Hello", blocks: vec![(b"TySh", tool.clone())], ..Raw::default() }
            .filled([0, 0, 255, 255]),
        // turned: pixels only
        Raw {
            rect: [0, 2, 3, 6],
            name: "Turned",
            blocks: vec![(
                b"TySh",
                text::tests::type_tool(
                    "Hi",
                    "F",
                    10.0,
                    [0.0; 3],
                    0,
                    [0.0, 1.0, -1.0, 0.0, 2.0, 3.0],
                    [0.0, -3.0, 5.0, 1.0],
                    false,
                ),
            )],
            ..Raw::default()
        }
        .filled([0, 0, 0, 255]),
        // text with nothing drawn yet still comes in
        Raw { name: "Blank", blocks: vec![(b"TySh", tool)], ..Raw::default() },
    ];
    let got = read(&raw_psd(w, h, &records, [0; 4])).unwrap();
    let [_, hello, turned, blank] = got.document.layers() else { panic!() };
    let t = hello.text.as_ref().unwrap();
    assert_eq!((t.text.as_str(), t.font.as_str(), t.style.as_str()), ("Hello", "Helvetica", "Bold"));
    assert_eq!(t.origin, [2.0, 0.0]);
    assert_eq!(t.color, [0.0, 0.0, 1.0]);
    assert_eq!(hello.name, "Hello");
    assert_eq!(&got.pixels[1].1[2 * 4..][..4], &[0, 0, 255, 255]);
    assert!(turned.text.is_none());
    assert!(blank.text.is_some());
    assert!(got.pixels[3].1.iter().all(|&v| v == 0));
}

/// Real PSDs, when a folder of them is given (`PSD_CORPUS=folder`, never committed): each one
/// reads or is turned away as a kind not read, and what reads holds together.
#[test]
fn a_corpus_of_real_psds_reads() {
    let Ok(root) = std::env::var("PSD_CORPUS") else { return };
    let mut files = Vec::new();
    let mut folders = vec![std::path::PathBuf::from(&root)];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap().flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
            if path.is_dir() {
                folders.push(path);
            } else if ext == "psd" || ext == "psb" {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty(), "no PSDs in {root}");
    let (mut read_ok, mut refused, mut layers, mut texts, mut backgrounds, mut masks) = (0, 0, 0, 0, 0, 0);
    let mut failures = Vec::new();
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        match read(&bytes) {
            Ok(got) => {
                read_ok += 1;
                let doc = &got.document;
                layers += doc.layers().len();
                texts += doc.layers().iter().filter(|l| l.text.is_some()).count();
                masks += doc.layers().iter().filter(|l| l.kind == LayerKind::Mask).count();
                backgrounds += usize::from(doc.background_visible);
                let size = doc.width() as usize * doc.height() as usize * 4;
                let painted = doc.layers().iter().filter(|l| l.kind != LayerKind::Group).count();
                if got.pixels.len() != painted || got.pixels.iter().any(|(_, p)| p.len() != size) {
                    failures.push(format!("{}: pixels don't match the layers", path.display()));
                }
                if let Ok(dump) = std::env::var("PSD_CORPUS_LIST") {
                    let mut out = format!("== {}\n", path.display());
                    if doc.background_visible {
                        out += &format!("   background {:?}\n", doc.background);
                    }
                    for l in doc.layers().iter().rev() {
                        let text = l.text.as_ref().map(|t| {
                            format!(
                                " text {:?} {} {} {:.1} {:?} w{:.0}",
                                t.text, t.font, t.style, t.size, t.origin, t.width
                            )
                        });
                        out += &format!(
                            "   {:?} {:?} {:?} vis={} op={:.2} lock={} alpha={}{}\n",
                            l.kind,
                            l.name,
                            l.blend,
                            l.visible,
                            l.opacity,
                            l.locked,
                            l.alpha_lock,
                            text.unwrap_or_default()
                        );
                    }
                    use std::io::Write;
                    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&dump).unwrap();
                    f.write_all(out.as_bytes()).unwrap();
                }
            }
            Err(PsdError::Unsupported(_)) => refused += 1,
            Err(e) => failures.push(format!("{}: {e:?}", path.display())),
        }
    }
    eprintln!(
        "{} files: {read_ok} read ({layers} layers, {texts} text, {masks} masks, {backgrounds} backgrounds), {refused} not read",
        files.len()
    );
    assert!(failures.is_empty(), "{} failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn thirty_two_bit_samples_undo_their_prediction_and_come_to_eight_bits() {
    // two floats a row: 0.5 and 1.0, laid out as byte planes, each byte the difference from
    // the one before it
    let (a, b) = (0.5f32.to_be_bytes(), 1.0f32.to_be_bytes());
    let planar = [a[0], b[0], a[1], b[1], a[2], b[2], a[3], b[3]];
    let mut row: Vec<u8> = planar
        .iter()
        .scan(0u8, |last, &v| {
            let d = v.wrapping_sub(*last);
            *last = v;
            Some(d)
        })
        .collect();
    undo_prediction(&mut row, 8, 4);
    assert_eq!(row, [a, b].concat());
    // colour through the sRGB curve (linear 0.5 shows as 188), transparency as it is
    assert_eq!(to8(&row, 4, true), [188, 255]);
    assert_eq!(to8(&row, 4, false), [128, 255]);
    // out of range and not a number: clamped, nothing
    let odd: Vec<u8> = [2.0f32, -1.0, f32::NAN].iter().flat_map(|v| v.to_be_bytes()).collect();
    assert_eq!(to8(&odd, 4, true), [255, 0, 0]);
    // 16 bits rounded; samples the row hasn't are none
    assert_eq!(to8(&[0xFF, 0xFF, 0x80, 0x00, 0x00, 0x7F], 2, true), [255, 128, 0]);
    let mut three = [9u8; 3];
    samples_to8(&[0x12, 0x34], 0, 2, true, &mut three);
    assert_eq!(three, [18, 0, 0]);
}

/// Every sample of `raw` (of `sample` bytes) in 8 bits.
fn to8(raw: &[u8], sample: usize, colour: bool) -> Vec<u8> {
    let mut out = vec![0; raw.len() / sample];
    samples_to8(raw, 0, sample, colour, &mut out);
    out
}

#[test]
fn bitmaps_indexed_colours_and_lab_come_to_rgb() {
    // set bits are black; a row's bits end at a byte, and a window into it starts at its bit
    let bits = |row: &[u8], skip: usize, n: usize| {
        let mut out = vec![7; n];
        samples_to8(row, skip, 0, false, &mut out);
        out
    };
    assert_eq!(bits(&[0b1010_0000], 0, 3), [0, 255, 0]);
    assert_eq!(bits(&[0b0100_0000], 0, 3), [255, 0, 255]);
    assert_eq!(bits(&[0b0000_0001, 0b1000_0000], 7, 2), [0, 0]);
    // an indexed file's table: its reds, greens and blues one after another
    let mut table = vec![0u8; 768];
    (table[5], table[256 + 5], table[512 + 5]) = (10, 20, 30);
    let (colour, sample, palette) = format_of(2, 8, &table).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(colour == Colour::Indexed && sample == 1);
    assert_eq!(palette[5], [10, 20, 30]);
    assert_eq!(format_of(2, 8, &[0; 10]).err(), Some(PsdError::Damaged("colour data")));
    // white, black and middle grey (L* 50 is sRGB 119)
    assert_eq!(lab_to_srgb(255, 128, 128), [255, 255, 255]);
    assert_eq!(lab_to_srgb(0, 128, 128), [0, 0, 0]);
    let grey = lab_to_srgb(128, 128, 128);
    assert!(grey.iter().all(|&c| (118..=120).contains(&c)), "{grey:?}");
    // duotones read as their grey, bitmaps only as bits; multichannel files not at all
    assert!(format_of(8, 8, &[]).is_ok_and(|(c, ..)| c == Colour::Grey));
    assert_eq!(format_of(0, 8, &[]).err(), Some(PsdError::Unsupported("bit depth")));
    assert_eq!(format_of(7, 8, &[]).err(), Some(PsdError::Unsupported("colour mode")));
}

#[test]
fn packbits_and_prediction_decode() {
    let mut out = Vec::new();
    // three literal bytes, then 9 repeated four times
    unpack(&[2, 1, 2, 3, 0xFD, 9], 7, &mut out);
    assert_eq!(out, [1, 2, 3, 9, 9, 9, 9]);
    let mut d = [1u8, 1, 1, 5, 0, 0];
    undo_prediction(&mut d, 3, 1);
    assert_eq!(d, [1, 2, 3, 5, 5, 5]);
}

/// Bytes in memory read as a file is, keeping count of the most asked for in one read.
struct Counted<'a> {
    bytes: &'a [u8],
    most: std::sync::atomic::AtomicU64,
}

impl Counted<'_> {
    fn new(bytes: &[u8]) -> Counted<'_> {
        Counted { bytes, most: std::sync::atomic::AtomicU64::new(0) }
    }

    fn most(&self) -> u64 {
        self.most.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Source for Counted<'_> {
    fn size(&self) -> u64 {
        self.bytes.size()
    }

    fn read_at(&self, at: u64, buf: &mut [u8]) -> bool {
        self.most.fetch_max(buf.len() as u64, std::sync::atomic::Ordering::Relaxed);
        self.bytes.read_at(at, buf)
    }
}

/// A layer's names and pixels as read, bottom first.
fn names_and_pixels(got: &Imported) -> Vec<(String, Vec<u8>)> {
    let px = |id: LayerId| got.pixels.iter().find(|(i, _)| *i == id).map(|(_, p)| p.clone()).unwrap_or_default();
    got.document.layers().iter().map(|l| (l.name.clone(), px(l.id))).collect()
}

#[test]
fn every_packing_depth_and_the_large_document_format_read_alike() {
    let (w, h) = (7u32, 5u32);
    let mut base = Raw { rect: [0, 0, 5, 7], name: "Base", ..Raw::default() }.filled([9, 8, 7, 255]);
    base.channels[1].1[3] = 200;
    // every sample different, partly off the canvas (from a row above it), half see-through
    let samples = |k: u8| (0..35u8).map(|i| i.wrapping_mul(37).wrapping_add(k * 50)).collect::<Vec<u8>>();
    let painted = Raw {
        rect: [-1, 2, 4, 9],
        channels: vec![
            (-1, samples(0)),
            (0, samples(1)),
            (1, samples(2)),
            (2, samples(3)),
            (-2, vec![0, 255, 128, 64]),
        ],
        mask: mask_data([1, 3, 3, 5], 255, 0, 20),
        name: "Painted",
        ..Raw::default()
    };
    let records = [base, painted];
    let mut first: Option<Vec<(String, Vec<u8>)>> = None;
    let mut read_ways = 0;
    for pack in [Pack::Raw, Pack::Rle, Pack::Zip, Pack::ZipPredicted] {
        for depth in [8, 16] {
            for big in [false, true] {
                for negative in [false, true] {
                    let make = Make { pack, depth, big, negative };
                    let bytes = make_psd(w, h, &records, [1, 2, 3, 255], &make);
                    let got = read(&bytes).unwrap_or_else(|e| panic!("{make:?}: {e:?}"));
                    let seen = names_and_pixels(&got);
                    match &first {
                        None => first = Some(seen),
                        Some(f) => assert_eq!(&seen, f, "{make:?}"),
                    }
                    let (_, _, flat) = flattened(&bytes).unwrap_or_else(|| panic!("{make:?}: no picture"));
                    assert_eq!(&flat[..4], &[1, 2, 3, 255], "{make:?}");
                    read_ways += 1;
                }
            }
        }
    }
    assert_eq!(read_ways, 32);
    let first = first.unwrap();
    let names: Vec<&str> = first.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["Base", "Painted", "Mask"]);
    // the painted layer's top left on the canvas is its sample at row 1, column 0 (premultiplied)
    let i = 7;
    let s = |k: u8| u16::from(samples(k)[i]);
    let a = s(0);
    let at = 2 * 4;
    let want: Vec<u8> = [1u8, 2, 3].iter().map(|&k| ((s(k) * a + 127) / 255) as u8).chain([a as u8]).collect();
    assert_eq!(&first[1].1[at..at + 4], &want[..]);
    // the mask's grey where it is, its outside colour elsewhere
    assert_eq!(first[2].1[(7 + 3) * 4..][..4], [0, 0, 0, 255]);
    assert_eq!(first[2].1[0..4], [255; 4]);
}

#[test]
fn a_layer_far_off_the_canvas_or_huge_brings_only_what_is_on_it() {
    let (w, h) = (8u32, 6u32);
    let mut base = Raw { rect: [0, 0, 6, 8], name: "Base", ..Raw::default() }.filled([9, 9, 9, 255]);
    base.channels[1].1[0] = 0;
    // a million pixels wide, ending on the canvas's left half: reading it holds a row of it at a
    // time, never the layer
    let wide = 1_000_000usize;
    let rows = 2;
    let alpha: Vec<u8> = (0..rows * wide).map(|i| if i % wide >= wide - 4 { 255 } else { 0 }).collect();
    let red = vec![200u8; rows * wide];
    let zero = vec![0u8; rows * wide];
    let huge = Raw {
        rect: [2, 4 - wide as i32, 4, 4],
        channels: vec![(-1, alpha), (0, red), (1, zero.clone()), (2, zero)],
        name: "Wide",
        pack: Some(Pack::Rle),
        ..Raw::default()
    };
    // wholly off the canvas, of no size, and wider than any layer is: left out
    let away = Raw { rect: [100, -100, 102, -98], name: "Away", ..Raw::default() }.filled([1, 2, 3, 255]);
    let none = Raw { rect: [3, 3, 3, 3], name: "None", ..Raw::default() }.filled([1, 2, 3, 255]);
    let too_wide = Raw {
        rect: [0, 0, 1, 2_000_000],
        channels: [-1, 0, 1, 2].map(|id| (id, vec![0u8; 2_000_000])).to_vec(),
        name: "Too wide",
        pack: Some(Pack::Rle),
        ..Raw::default()
    };
    let bytes = raw_psd(w, h, &[base, huge, away, none, too_wide], [0; 4]);
    let counted = Counted::new(&bytes);
    let art = open(&counted).unwrap();
    let names: Vec<&str> = art.document.layers().iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Base", "Wide"]);
    let id = art.document.layers()[1].id;
    let px = art.pixels(id).unwrap();
    for y in 0..h as usize {
        for x in 0..w as usize {
            let want = if (2..4).contains(&y) && x < 4 { [200, 0, 0, 255] } else { [0; 4] };
            assert_eq!(px[(y * w as usize + x) * 4..][..4], want, "({x}, {y})");
        }
    }
    // no read took more than a buffer's worth, though the layer's rows are a megabyte each
    assert!(counted.most() <= AHEAD as u64, "a read of {} bytes", counted.most());
}

#[test]
fn odd_structures_read_as_the_file_means_them() {
    let (w, h) = (4, 4);
    let mut base = Raw { rect: [0, 0, 4, 4], name: "Base", ..Raw::default() }.filled([0, 0, 0, 255]);
    base.channels[1].1[3] = 9;
    let lsct = |kind: u32| (b"lsct", u32s(&[kind]));
    let lsdk = |kind: u32| (b"lsdk", u32s(&[kind]));
    let luni = |name: &str| {
        let units: Vec<u16> = name.encode_utf16().collect();
        let mut out = u32s(&[units.len() as u32]);
        out.extend(units.iter().flat_map(|u| u.to_be_bytes()));
        out
    };
    let dot = |name: &'static str| Raw { rect: [0, 0, 1, 1], name, ..Raw::default() }.filled([5, 5, 5, 255]);
    let records = [
        base,
        // a group's start with no end below it: an empty group
        Raw { name: "Lonely", blocks: vec![lsct(1)], ..Raw::default() },
        // a group in a group, their markers as some write them
        Raw { name: "</Layer group>", blocks: vec![lsct(3)], ..Raw::default() },
        Raw { name: "</Layer group>", blocks: vec![lsdk(3)], ..Raw::default() },
        Raw {
            // its name in Unicode, an unknown block of odd length before it, and each block's data
            // padded to an even length as some write them
            name: "x",
            blocks: vec![(b"abcd", vec![1, 2, 3]), (b"luni", luni("線画 ✏️"))],
            pad: 2,
            flags: 1,
            blend: b"pass",
            ..dot("x")
        },
        Raw {
            // a name not in UTF-8, each block padded to four
            pascal: Some(vec![0x83, 0x8C, b'A', 0xFF]),
            blocks: vec![(b"abcd", vec![1]), (b"lspf", u32s(&[1]))],
            pad: 4,
            blend: b"xxxx",
            ..dot("")
        },
        Raw { name: "Inner", blocks: vec![lsdk(1)], ..Raw::default() },
        Raw { name: "Outer", blocks: vec![lsct(2)], ..Raw::default() },
        // a group's end with no start above it: its layers come in outside any group
        Raw { name: "</Layer group>", blocks: vec![lsct(3)], ..Raw::default() },
        dot("Stray"),
        // packed in a way no writer packs: kept, clear
        Raw { pack: Some(Pack::Unknown), ..dot("Unknown packing") },
    ];
    for negative in [false, true] {
        let bytes = make_psd(w, h, &records, [0; 4], &Make { negative, ..Make::default() });
        let got = read(&bytes).unwrap();
        let doc = &got.document;
        let names: Vec<(&str, LayerKind)> = doc.layers().iter().map(|l| (l.name.as_str(), l.kind)).collect();
        assert_eq!(
            names,
            [
                ("Base", LayerKind::Paint),
                ("Lonely", LayerKind::Group),
                ("線画 ✏️", LayerKind::Paint),
                ("A", LayerKind::Paint),
                ("Inner", LayerKind::Group),
                ("Outer", LayerKind::Group),
                ("Stray", LayerKind::Paint),
                ("Unknown packing", LayerKind::Paint),
            ]
        );
        let [_, lonely, unicode, odd, inner, outer, stray, unknown] = doc.layers() else { panic!() };
        assert_eq!((lonely.parent, stray.parent, unknown.parent), (None, None, None));
        assert_eq!((unicode.parent, odd.parent), (Some(inner.id), Some(inner.id)));
        assert_eq!((inner.parent, outer.parent), (Some(outer.id), None));
        // locked by its flags, or its locks; unknown blend modes normal
        assert!(unicode.alpha_lock && odd.alpha_lock);
        assert_eq!((unicode.blend, odd.blend), (BlendMode::Normal, BlendMode::Normal));
        assert_eq!(&got.pixels[2].1[..4], &[5, 5, 5, 255]);
        let unknown_px = got.pixels.iter().find(|(id, _)| *id == unknown.id).unwrap();
        assert!(unknown_px.1.iter().all(|&v| v == 0));
    }
}

#[test]
fn a_file_cut_short_or_with_a_damaged_channel_brings_what_reads() {
    let (w, h) = (6u32, 4u32);
    let mut base = Raw { rect: [0, 0, 4, 6], name: "Base", ..Raw::default() }.filled([50, 60, 70, 255]);
    base.channels[1].1[0] = 0;
    let top = Raw { rect: [0, 0, 4, 6], name: "Top", ..Raw::default() }.filled([200, 0, 0, 255]);
    let make = Make { pack: Pack::Rle, ..Make::default() };
    let bytes = make_psd(w, h, &[base, top], [1, 1, 1, 255], &make);
    let whole = read(&bytes).unwrap();
    // cut inside the top layer's last channel: both layers come, the top one as far as it reads
    let info_end = bytes.len() - 2 - (w * h * 4) as usize - 4;
    let cut = &bytes[..info_end - 3];
    let got = read(cut).unwrap();
    let names: Vec<&str> = got.document.layers().iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Base", "Top"]);
    assert_eq!(got.pixels[0].1, whole.pixels[0].1);
    assert_eq!(&got.pixels[1].1[..4], &[200, 0, 0, 255]);
    // a row's count far past what a row can take: that channel stops there, the rest reads
    let mut bad = bytes.clone();
    let counts = bytes.windows(8).position(|s| *s == [0u8, 1, 0, 2, 0, 2, 0, 2]).unwrap();
    bad[counts + 2..counts + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
    let got = read(&bad).unwrap();
    assert_eq!(got.document.layers().len(), 2);
    assert_eq!(got.pixels[1].1, whole.pixels[1].1);
}

#[test]
fn past_the_layer_cap_the_whole_file_is_turned_away() {
    // no part of it comes in, neither the layers up to the cap nor the rest merged; masks count as
    // layers, groups and a bottom layer that is the background don't
    let (w, h) = (4, 4);
    let lsct = |kind: u32| (b"lsct", u32s(&[kind]));
    let dot = |name: &'static str| Raw { rect: [0, 0, 1, 1], name, ..Raw::default() }.filled([5, 5, 5, 255]);
    let records = |bottom: Raw<'static>| {
        let mut masked = dot("Masked");
        masked.mask = mask_data([0, 0, 1, 1], 0, 0, 20);
        masked.channels.push((-2, vec![255]));
        [
            bottom,
            Raw { name: "</Layer group>", blocks: vec![lsct(3)], ..Raw::default() },
            dot("One"),
            dot("Two"),
            masked,
            Raw { name: "Group", blocks: vec![lsct(1)], ..Raw::default() },
        ]
    };
    let mut two = Raw { rect: [0, 0, 4, 4], name: "Base", ..Raw::default() }.filled([1, 2, 3, 255]);
    two.channels[1].1[0] = 9;
    let plain = Raw { rect: [0, 0, 4, 4], name: "Base", ..Raw::default() }.filled([1, 2, 3, 255]);
    // five with pixels: the base, three layers and a mask; four when the base is the background
    for (bottom, count) in [(two, 5), (plain, 4)] {
        let bytes = raw_psd(w, h, &records(bottom), [0; 4]);
        let opened = |cap: usize| open_within(parse(bytes.as_slice()).unwrap(), cap).map(|a| a.document);
        let doc = opened(count).unwrap();
        assert_eq!(doc.pixel_layer_ids().count(), count);
        assert_eq!(opened(count - 1).err(), Some(PsdError::TooManyLayers(count - 1)));
        assert_eq!(opened(1).err(), Some(PsdError::TooManyLayers(1)));
    }
}

#[test]
fn a_canvas_no_device_holds_is_turned_away_before_anything_is_read() {
    // a few bytes saying a canvas of a gigabyte a layer: turned away, nothing made for it
    let head = |w: u32, h: u32| {
        let mut out = b"8BPS".to_vec();
        out.extend(1u16.to_be_bytes());
        out.extend([0; 6]);
        out.extend(3u16.to_be_bytes());
        out.extend(u32s(&[h, w]));
        out.extend(8u16.to_be_bytes());
        out.extend(3u16.to_be_bytes());
        out.extend(u32s(&[0, 0, 0]));
        out.extend(1u16.to_be_bytes());
        out
    };
    // (layers may take 2 GiB here: a layer of 1 GiB leaves no room once four are kept back)
    assert_eq!(read(&head(16384, 16384)).err(), Some(PsdError::TooLarge));
    assert_eq!(read(&head(16385, 4)).err(), Some(PsdError::TooLarge));
    assert_eq!(read(&head(0, 4)).err(), Some(PsdError::Damaged("size")));
    // a small one says what it is: damaged (its picture is cut short)
    assert_eq!(read(&head(64, 64)).err(), Some(PsdError::Damaged("picture")));
}

#[test]
fn a_file_on_disk_reads_as_the_same_bytes_in_memory_do() {
    let (w, h) = (5u32, 3u32);
    let mut base = Raw { rect: [0, 0, 3, 5], name: "Base", ..Raw::default() }.filled([9, 8, 7, 255]);
    base.channels[2].1[4] = 1;
    let top = Raw { rect: [1, 1, 3, 4], name: "Top", ..Raw::default() }.filled([1, 200, 3, 128]);
    let bytes = make_psd(w, h, &[base, top], [4, 5, 6, 255], &Make { pack: Pack::Zip, ..Make::default() });
    let path = std::env::temp_dir().join(format!("procraft-psd-test-{}.psd", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let file = FileSource::open(&path).unwrap();
    let art = open(&file).unwrap();
    let from_memory = read(&bytes).unwrap();
    assert_eq!(art.document, from_memory.document);
    for (id, px) in &from_memory.pixels {
        assert_eq!(&art.pixels(*id).unwrap(), px);
    }
    assert_eq!(art.flattened(), flattened(&bytes).map(|(_, _, p)| p));
    drop(art);
    drop(file);
    let _ = std::fs::remove_file(path);
}

/// Hand-made files of every kind read here, for the mutations below to start from: groups (their
/// markers both ways, one unbalanced), masks, Unicode names, text, locks; every packing, both
/// depths, PSD and PSB; and one written as the app's own export writes it, with a profile.
fn seeds() -> Vec<Vec<u8>> {
    let (w, h) = (24u32, 16u32);
    let tool = super::text::tests::type_tool(
        "Hi",
        "Helvetica-Bold",
        10.0,
        [0.0, 0.0, 1.0],
        0,
        [1.0, 0.0, 0.0, 1.0, 2.0, 3.0],
        [0.0, -3.0, 5.0, 1.0],
        false,
    );
    let luni = |name: &str| {
        let units: Vec<u16> = name.encode_utf16().collect();
        let mut out = u32s(&[units.len() as u32]);
        out.extend(units.iter().flat_map(|u| u.to_be_bytes()));
        out
    };
    let records = || {
        let mut base = Raw { rect: [0, 0, 16, 24], name: "Base", ..Raw::default() }.filled([240, 240, 240, 255]);
        base.channels[1].1[5] = 3;
        let gradient = |n: usize, k: u8| (0..n).map(|i| (i as u8).wrapping_mul(k)).collect::<Vec<u8>>();
        let mut masked = Raw {
            rect: [-2, 3, 10, 30],
            channels: vec![
                (-1, gradient(324, 3)),
                (0, gradient(324, 5)),
                (1, gradient(324, 7)),
                (2, gradient(324, 11)),
            ],
            mask: mask_data([1, 1, 9, 9], 0, 0, 20),
            name: "Masked",
            blend: b"mul ",
            clipping: 1,
            ..Raw::default()
        };
        masked.channels.push((-2, gradient(64, 13)));
        vec![
            base,
            Raw { name: "</Layer group>", blocks: vec![(b"lsct", u32s(&[3]))], ..Raw::default() },
            masked,
            Raw { name: "</Layer group>", blocks: vec![(b"lsdk", u32s(&[3]))], ..Raw::default() },
            Raw {
                rect: [4, 4, 8, 12],
                name: "Text",
                blocks: vec![(b"TySh", tool.clone()), (b"luni", luni("文字"))],
                pad: 2,
                ..Raw::default()
            }
            .filled([0, 0, 255, 255]),
            Raw { name: "Inner", blocks: vec![(b"lsdk", u32s(&[1])), (b"lspf", u32s(&[4]))], ..Raw::default() },
            Raw { rect: [10, 10, 14, 20], name: "Top", flags: 1, ..Raw::default() }.filled([10, 200, 30, 200]),
            Raw { name: "Outer", blocks: vec![(b"lsct", u32s(&[2]))], ..Raw::default() },
            Raw { name: "Lonely", blocks: vec![(b"lsct", u32s(&[1]))], ..Raw::default() },
        ]
    };
    let mut out = Vec::new();
    for make in [
        Make::default(),
        Make { pack: Pack::Rle, ..Make::default() },
        Make { pack: Pack::Zip, depth: 16, ..Make::default() },
        Make { pack: Pack::ZipPredicted, big: true, ..Make::default() },
        Make { pack: Pack::Rle, depth: 16, big: true, negative: true },
    ] {
        out.push(make_psd(w, h, &records(), [5, 6, 7, 255], &make));
    }
    let rgba: Vec<u8> = (0..w * h).flat_map(|i| [(i % 251) as u8, 40, 90, if i % 3 == 0 { 0 } else { 255 }]).collect();
    let grey: Vec<u8> = (0..w * h).map(|i| (i % 256) as u8).collect();
    let layers = [
        PsdLayer {
            name: "Inked",
            kind: PsdKind::Pixels { rgba: &rgba, mask: Some(&grey) },
            opacity: 0.8,
            visible: true,
            blend: BlendMode::Screen,
            clipped: false,
        },
        PsdLayer {
            name: "Base",
            kind: PsdKind::Pixels { rgba: &rgba, mask: None },
            opacity: 1.0,
            visible: true,
            blend: BlendMode::Normal,
            clipped: false,
        },
    ];
    out.push(psd(w, h, 132.0, &crate::icc::icc(Profile::DisplayP3), &layers, &rgba, None));
    out
}

/// `good` changed `cases` ways: cut short, bytes flipped or set, numbers (lengths, counts, sizes)
/// set to their extremes, and runs taken out or repeated.
fn mutations(good: &[u8], cases: usize, mut seed: u64) -> impl Iterator<Item = Vec<u8>> + '_ {
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let n = good.len() as u64;
    (0..cases).map(move |i| {
        let mut bad = good.to_vec();
        let at = (next() % n) as usize;
        match i % 7 {
            0 => bad.truncate(at),
            1 => bad[at] ^= (next() as u8) | 1,
            2 => {
                for _ in 0..1 + next() % 8 {
                    let at = (next() % n) as usize;
                    bad[at] = next() as u8;
                }
            }
            3 => {
                const EXTREMES: [u32; 8] = [0, 1, 2, 0x7FFF_FFFF, 0xFFFF_FFFF, 0x8000_0000, 0x0001_0000, 0x00FF_FFFF];
                let v = EXTREMES[(next() % 8) as usize].to_be_bytes();
                let end = (at + 4).min(bad.len());
                bad[at..end].copy_from_slice(&v[..end - at]);
            }
            4 => {
                const EXTREMES: [u16; 8] = [0, 1, 2, 3, 7, 0x7FFF, 0x8000, 0xFFFF];
                let v = EXTREMES[(next() % 8) as usize].to_be_bytes();
                let end = (at + 2).min(bad.len());
                bad[at..end].copy_from_slice(&v[..end - at]);
            }
            5 => {
                let len = (1 + next() % 64) as usize;
                bad.drain(at..(at + len).min(bad.len()));
            }
            _ => {
                let len = (1 + next() % 64) as usize;
                let run = bad[at..(at + len).min(bad.len())].to_vec();
                let to = (next() % n) as usize;
                bad.splice(to..to, run);
            }
        }
        bad
    })
}

/// Everything there is to read of `bytes`, as an import reads it: the document, each layer's
/// pixels, the flattened picture. (Canvases of more than 4 million pixels are only opened, so the
/// run keeps to a few megabytes a case.)
fn read_all(bytes: &[u8]) -> u64 {
    let counted = Counted::new(bytes);
    let small = head(bytes).is_ok_and(|h| u64::from(h.format.width) * u64::from(h.format.height) <= 1 << 22);
    if !small {
        let _ = head(bytes);
        return counted.most();
    }
    if let Ok(art) = open(&counted) {
        for id in art.layers_with_pixels() {
            let _ = art.pixels(id);
        }
        let _ = art.flattened();
        assert!(art.document.pixel_layer_ids().count() <= crate::document::MOST_LAYERS);
    }
    let _ = flattened(bytes);
    counted.most()
}

#[test]
fn psds_changed_every_way_never_panic_nor_read_unbounded() {
    // `PSD_FUZZ_CASES=n` for more cases a seed than the 4000 every run makes
    let cases = std::env::var("PSD_FUZZ_CASES").ok().and_then(|n| n.parse().ok()).unwrap_or(4000);
    let seeds = seeds();
    for (i, good) in seeds.iter().enumerate() {
        assert!(read(good).is_ok(), "seed {i} doesn't read");
    }
    let mut ran = 0usize;
    let mut failed = Vec::new();
    let mut most = 0;
    for (i, good) in seeds.iter().enumerate() {
        for (k, bad) in mutations(good, cases, 0x9E37_79B9_7F4A_7C15 ^ (i as u64 + 1)).enumerate() {
            ran += 1;
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_all(&bad))) {
                Ok(read) => most = most.max(read),
                Err(_) => failed.push(format!("seed {i} case {k}")),
            }
        }
    }
    eprintln!("{ran} changed files read: none panicked; the most read at once was {most} bytes");
    assert!(failed.is_empty(), "{} of {ran} panicked: {}", failed.len(), failed.join(", "));
    assert!(most <= AHEAD as u64 * 2, "a read of {most} bytes");
}

#[test]
fn the_flattened_picture_reads_however_it_is_packed_and_makes_a_thumbnail_on_white() {
    let (w, h) = (300u32, 200u32);
    // half see-through red on the left, opaque blue on the right
    let px = |x: u32| if x < w / 2 { [255u8, 0, 0, 128] } else { [0, 0, 255, 255] };
    let planes: Vec<Vec<u8>> = (0..4).map(|c| (0..h).flat_map(|_| (0..w).map(move |x| px(x)[c])).collect()).collect();
    // a file without layers, its picture cut off to be written each way
    let mut head = raw_psd(w, h, &[], [0; 4]);
    head.truncate(head.len() - 2 - (w * h * 4) as usize);
    let rows = |p: &Vec<u8>| p.chunks(w as usize).map(<[u8]>::to_vec).collect::<Vec<_>>();
    for pack in [Pack::Raw, Pack::Rle, Pack::Zip, Pack::ZipPredicted] {
        let mut bytes = head.clone();
        match pack {
            Pack::Raw | Pack::Unknown => {
                bytes.extend(0u16.to_be_bytes());
                bytes.extend(planes.concat());
            }
            Pack::Rle => {
                bytes.extend(1u16.to_be_bytes());
                let packed: Vec<Vec<u8>> = planes.iter().flat_map(rows).map(|r| packbits(&r)).collect();
                for p in &packed {
                    bytes.extend((p.len() as u16).to_be_bytes());
                }
                bytes.extend(packed.concat());
            }
            Pack::Zip => {
                bytes.extend(2u16.to_be_bytes());
                bytes.extend(miniz_oxide::deflate::compress_to_vec_zlib(&planes.concat(), 6));
            }
            Pack::ZipPredicted => {
                bytes.extend(3u16.to_be_bytes());
                let deltas: Vec<u8> = planes
                    .iter()
                    .flat_map(rows)
                    .flat_map(|r| {
                        let mut last = 0u8;
                        r.into_iter().map(move |v| {
                            let d = v.wrapping_sub(last);
                            last = v;
                            d
                        })
                    })
                    .collect();
                bytes.extend(miniz_oxide::deflate::compress_to_vec_zlib(&deltas, 6));
            }
        }
        let (fw, fh, flat) = flattened(&bytes).unwrap_or_else(|| panic!("{pack:?}"));
        assert_eq!((fw, fh), (w, h));
        assert_eq!(flat[..4], [255, 0, 0, 128], "{pack:?}");
        assert_eq!(flat[(w as usize - 1) * 4..][..4], [0, 0, 255, 255], "{pack:?}");
        // the one layer it brings, and its thumbnail on white made a row at a time
        let art = open(bytes.as_slice()).unwrap();
        assert_eq!(art.document.layers().len(), 1);
        let (tw, th, thumb) = art.thumbnail(150).unwrap();
        assert_eq!((tw, th), (150, 100));
        assert_eq!(thumb[..4], [255, 127, 127, 255], "{pack:?}");
        assert_eq!(thumb[(tw as usize - 1) * 4..][..4], [0, 0, 255, 255], "{pack:?}");
        assert_eq!(thumb[((th as usize - 1) * tw as usize) * 4..][..4], [255, 127, 127, 255], "{pack:?}");
    }
}
