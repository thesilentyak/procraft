//! Big artworks open and export within the budgets the app sets on the devices they are drawn on:
//! what the heap and the GPU hold at their fullest, while the artwork is opened and shown and while
//! it is exported in each format, from the open canvas and from the gallery (opened only to be
//! exported). A4 at 300 dpi of 39 layers on a device of 4 GB, its files the same as those written
//! with every layer on the GPU; and A3 at 400 dpi of 7 layers on a tablet of 6 GB, where 0.0.7 was
//! stopped for memory opening or sharing it. (A test of its own, as it counts every allocation the
//! program makes; the GPU's memory is told on Direct3D 12, `WGPU_BACKEND=dx12`, else it is
//! skipped.) `BIG_ARTWORK_DIR=folder` keeps the artworks there, to be used again.

// (the helpers too are the test's)
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use procraft_core::CanvasSession;
use procraft_engine::document::Document;
use procraft_gpu::{Gpu, wgpu};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// An artwork measured: its size and layers, the memory of the device it opens on (the app sets
/// the budgets from it: half for layers, a quarter for their textures) and that device's screen,
/// the window the canvas is shown in.
struct Art {
    name: &'static str,
    width: u32,
    height: u32,
    layers: usize,
    ram: u64,
    screen: (u32, u32),
}

impl Art {
    /// One layer's pixels, bytes.
    fn layer_bytes(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height) * 4
    }
}

/// A4 at 300 dpi, of 39 layers, on a device of 4 GB.
const A4: Art = Art { name: "A4", width: 2480, height: 3508, layers: 39, ram: 4 << 30, screen: (1200, 2000) };

/// A3 at 400 dpi, of 7 layers, on a Galaxy Tab S7 (MemTotal 5,784,256 kB; a 1600 x 2560 screen),
/// where 0.0.7 was stopped for memory as it opened it (1.8 GB of the heap and 2.1 GB of the GPU's)
/// or shared it in any format.
const A3: Art = Art { name: "A3", width: 4677, height: 6614, layers: 7, ram: 5_784_256 * 1024, screen: (1600, 2560) };

/// The most the A3 may hold, the heap and the GPU together, opened and shown and shared in any
/// format, or shared from the gallery: what that tablet leaves the app (2.5 GB), less what the app
/// holds before an artwork opens (0.4 GB in its log).
const A3_MOST: u64 = 2100 << 20;

/// The measured steps one at a time: the heap's profiler counts the whole program.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The GPU's memory at its fullest (what its allocator holds), sampled while something is
/// measured.
static GPU_PEAK: AtomicU64 = AtomicU64::new(0);
/// The heap and the GPU together at their fullest, as sampled.
static BOTH_PEAK: AtomicU64 = AtomicU64::new(0);

const MB: f64 = (1u64 << 20) as f64;

/// The most opening and showing the artwork may hold beyond the textures' budget, in layers'
/// worth: the layers kept compressed off the GPU (what of them doesn't pack), the few being read
/// and the buffers they go up through. (All of them at once, as before, came to 26 more.)
const OPENING_MORE: u64 = 18;

/// The most an export may hold, in layers' worth: the picture, the layer being written, what is
/// made of it and its copy on its way back from the GPU. (Every layer at once came to 141.)
const EXPORTING: u64 = 6;

/// What one step held at its fullest beyond what was held before it.
#[derive(Clone, Copy, Debug)]
struct Peak {
    heap: u64,
    gpu: u64,
    both: u64,
}

impl Peak {
    fn total(&self) -> u64 {
        self.both.max(self.heap.max(self.gpu))
    }
}

/// Samples the GPU's allocator every 2 ms, and the heap with it, until `stop`.
fn sampler(gpu: Gpu, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let now = gpu.device.generate_allocator_report().map_or(0, |r| r.total_reserved_bytes);
            GPU_PEAK.fetch_max(now, Ordering::Relaxed);
            let heap = dhat::HeapStats::get().curr_bytes as u64;
            BOTH_PEAK.fetch_max(heap + now, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(2));
        }
    })
}

/// Runs `step` and returns what it held at its fullest beyond what was held when it began (the
/// heap's own count, and the GPU's as sampled).
fn measure<T>(gpu: &Gpu, step: impl FnOnce() -> T) -> (T, Peak) {
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let gpu_before = gpu.device.generate_allocator_report().map_or(0, |r| r.total_reserved_bytes);
    GPU_PEAK.store(gpu_before, Ordering::Relaxed);
    BOTH_PEAK.store(gpu_before, Ordering::Relaxed);
    // the heap counted from here: the profiler counts what is allocated after its start
    let profiler = dhat::Profiler::builder().testing().build();
    let stop = Arc::new(AtomicBool::new(false));
    let thread = sampler(gpu.clone(), stop.clone());
    let out = step();
    stop.store(true, Ordering::Relaxed);
    let _ = thread.join();
    let heap_peak = dhat::HeapStats::get().max_bytes as u64;
    drop(profiler);
    let gpu_peak = GPU_PEAK.load(Ordering::Relaxed).saturating_sub(gpu_before);
    let both = BOTH_PEAK.load(Ordering::Relaxed).saturating_sub(gpu_before);
    (out, Peak { heap: heap_peak, gpu: gpu_peak, both })
}

/// A layer's pixels, premultiplied: soft blobs of colour across the whole canvas with a little
/// grain, as painted layers are (each a different picture).
fn layer_pixels(art: &Art, seed: u64) -> Vec<u8> {
    let (w, h) = (art.width as usize, art.height as usize);
    let mut px = vec![0u8; w * h * 4];
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    // the first layer is painted all over; the others in blobs spread over the canvas
    let blobs = if seed == 0 { 1 } else { 5 };
    for b in 0..blobs {
        let (cx, cy, rx, ry) = if seed == 0 {
            (w as f32 / 2.0, h as f32 / 2.0, w as f32, h as f32)
        } else {
            let r = next();
            let cx = (r % w as u64) as f32;
            let cy = ((r >> 20) % h as u64) as f32;
            let rx = 150.0 + ((r >> 40) % 700) as f32;
            let ry = 150.0 + ((r >> 50) % 900) as f32;
            (cx, cy, rx, ry)
        };
        let c = next();
        let colour = [(c & 0xFF) as f32, ((c >> 8) & 0xFF) as f32, ((c >> 16) & 0xFF) as f32];
        let alpha = if seed == 0 { 255.0 } else { 120.0 + ((c >> 24) % 136) as f32 };
        let y0 = (cy - ry).max(0.0) as usize;
        let y1 = ((cy + ry) as usize).min(h);
        let x0 = (cx - rx).max(0.0) as usize;
        let x1 = ((cx + rx) as usize).min(w);
        for y in y0..y1 {
            let dy = (y as f32 - cy) / ry;
            for xx in x0..x1 {
                let dx = (xx as f32 - cx) / rx;
                let d = dx * dx + dy * dy;
                if d >= 1.0 {
                    continue;
                }
                // soft at the edge, and a little grain
                let grain = (next() & 0x1F) as f32 - 16.0;
                let a = (alpha * (1.0 - d * d) + grain * 0.5).clamp(0.0, 255.0) / 255.0;
                let shade = 1.0 - 0.3 * (xx as f32 / w as f32) + (b as f32) * 0.01;
                let i = (y * w + xx) * 4;
                let p = &mut px[i..i + 4];
                // over what is there
                let keep = 1.0 - a;
                for k in 0..3 {
                    let v = (colour[k] * shade).clamp(0.0, 255.0) * a + f32::from(p[k]) * keep;
                    p[k] = v.round().clamp(0.0, 255.0) as u8;
                }
                p[3] = (a * 255.0 + f32::from(p[3]) * keep).round().clamp(0.0, 255.0) as u8;
                for k in 0..3 {
                    p[k] = p[k].min(p[3]);
                }
            }
        }
    }
    px
}

/// The PNG an artwork's folder keeps a layer in (its pixels as they are).
fn png(art: &Art, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, art.width, art.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(rgba).unwrap();
    writer.finish().unwrap();
    out
}

/// Writes the artwork's folder into `dir`: its file and a PNG for each layer.
fn write_artwork(art: &Art, dir: &Path) {
    let mut doc = Document::new(art.width, art.height);
    let mut ids = vec![doc.current()];
    for _ in 1..art.layers {
        ids.push(doc.add_layer());
    }
    std::fs::create_dir_all(dir.join("layers")).unwrap();
    let file = serde_json::json!({
        "format": "procraft-artwork",
        "version": 1,
        "name": "Big",
        "document": doc,
    });
    std::fs::write(dir.join("artwork.json"), serde_json::to_vec(&file).unwrap()).unwrap();
    // a few at a time
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..6 {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(id) = ids.get(i) else { break };
                    let bytes = png(art, &layer_pixels(art, i as u64));
                    std::fs::write(dir.join("layers").join(format!("{}.png", id.0)), bytes).unwrap();
                }
            });
        }
    });
}

/// Shows the session as the screen would, a frame at a time, until it wants no more.
fn show(art: &Art, s: &CanvasSession, gpu: &Gpu) {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("screen"),
        size: wgpu::Extent3d { width: art.screen.0, height: art.screen.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    s.show_in(art.screen.0, art.screen.1);
    for _ in 0..20_000 {
        let again = s.draw(
            &view,
            wgpu::TextureFormat::Rgba8Unorm,
            art.screen,
            procraft_engine::profile::Profile::Srgb,
            Instant::now(),
        );
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        if again.is_none_or(|d| d > Duration::from_millis(100)) && !s.wants_frame() {
            return;
        }
    }
}

/// The formats exported, by name, each written by the session into a file in `dir`.
const FORMATS: [&str; 6] = ["PNG", "JPEG", "PSD", "PDF", "TIFF", "package"];

/// Those exported from the gallery too: the flattened picture and the layered files (what the
/// others hold more is told from the canvas).
const GALLERY_FORMATS: [&str; 3] = ["PNG", "PSD", "package"];

fn export(s: &CanvasSession, format: &str, dir: &Path) -> PathBuf {
    let path = dir.join(format!("Big.{format}"));
    let p = path.to_string_lossy().into_owned();
    match format {
        "PNG" => s.export_png(p),
        "JPEG" => s.export_jpeg(p),
        "PSD" => s.export_psd(p),
        "PDF" => s.export_pdf(p, false, 100),
        "TIFF" => s.export_tiff(p),
        _ => s.export_package(p, 7, None),
    }
    .unwrap_or_else(|e| panic!("{format}: {e}"));
    path
}

fn mb(b: u64) -> f64 {
    b as f64 / MB
}

fn line(art: &Art, what: &str, p: Peak) {
    eprintln!(
        "{what:<26} heap {:7.0} MB | GPU {:7.0} MB | together {:7.0} MB ({:4.1} layers)",
        mb(p.heap),
        mb(p.gpu),
        mb(p.both),
        p.total() as f64 / art.layer_bytes() as f64
    );
}

/// A device whose GPU's memory can be told, the one offscreen exports use too; none (the test
/// skipped) without one.
fn measured_gpu() -> Option<Gpu> {
    let gpu = match Gpu::headless() {
        Ok(gpu) => gpu,
        Err(e) => {
            eprintln!("skipped: {e}");
            return None;
        }
    };
    if gpu.device.generate_allocator_report().is_none() {
        eprintln!("skipped: the GPU's memory can't be told");
        return None;
    }
    procraft_core::keep_device(&gpu);
    Some(gpu)
}

/// `art` written into its folder (kept there to be used again with `BIG_ARTWORK_DIR`), an empty
/// folder for what is exported, and the budgets the app sets for its device: the root folder, the
/// artwork's and the exports'.
fn prepared(art: &Art) -> (PathBuf, PathBuf, PathBuf) {
    let root = match std::env::var("BIG_ARTWORK_DIR") {
        Ok(d) => PathBuf::from(d),
        Err(_) => std::env::temp_dir().join(format!("procraft-big-artwork-{}-{}", art.name, std::process::id())),
    };
    let dir = root.join(art.name);
    if !dir.join("artwork.json").exists() {
        let t = Instant::now();
        write_artwork(art, &dir);
        eprintln!("{} written in {:.1} s", art.name, t.elapsed().as_secs_f64());
    }
    let out = root.join(format!("{} out", art.name));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    procraft_core::set_layer_budget(art.ram / 2);
    procraft_core::set_resident_budget(art.ram / 4);
    eprintln!("budgets: layers {:.0} MB, their textures {:.0} MB", mb(art.ram / 2), mb(art.ram / 4));
    (root, dir, out)
}

#[test]
fn a_big_artwork_opens_and_exports_within_its_budget() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(gpu) = measured_gpu() else { return };
    let (root, art, out) = prepared(&A4);
    let art_path = art.to_string_lossy().into_owned();
    let mut peaks = Vec::new();

    // opened and shown, as tapping it in the gallery does
    let t = Instant::now();
    let (s, open) = measure(&gpu, || {
        let s = CanvasSession::open(art_path.clone()).unwrap();
        s.use_gpu(gpu.clone());
        assert!(s.canvas_made());
        show(&A4, &s, &gpu);
        s
    });
    line(&A4, &format!("opened ({:.1} s)", t.elapsed().as_secs_f64()), open);
    peaks.push(("opened".to_owned(), open));
    let mut shown_files = Vec::new();
    for format in FORMATS {
        let t = Instant::now();
        let (path, p) = measure(&gpu, || export(&s, format, &out));
        line(&A4, &format!("{format} ({:.1} s)", t.elapsed().as_secs_f64()), p);
        peaks.push((format!("{format} from the canvas"), p));
        shown_files.push(std::fs::read(&path).unwrap());
        let _ = std::fs::remove_file(&path);
    }
    drop(s);

    // from the gallery: opened only to be exported
    let mut gallery_files = Vec::new();
    for format in GALLERY_FORMATS {
        let t = Instant::now();
        let (path, p) = measure(&gpu, || {
            let s = CanvasSession::open(art_path.clone()).unwrap();
            assert!(s.prepare_offscreen());
            export(&s, format, &out)
        });
        line(&A4, &format!("gallery {format} ({:.1} s)", t.elapsed().as_secs_f64()), p);
        peaks.push((format!("{format} from the gallery"), p));
        gallery_files.push(std::fs::read(&path).unwrap());
        let _ = std::fs::remove_file(&path);
    }

    // with no budget (every layer on the GPU), for the files as they should be
    procraft_core::set_resident_budget(0);
    let s = CanvasSession::open(art_path.clone()).unwrap();
    s.use_gpu(gpu.clone());
    show(&A4, &s, &gpu);
    for (i, format) in FORMATS.iter().enumerate() {
        let path = export(&s, format, &out);
        let whole = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(shown_files[i] == whole, "{format} from the canvas differs from the file with every layer on the GPU");
        if let Some(g) = GALLERY_FORMATS.iter().position(|f| f == format) {
            assert!(
                gallery_files[g] == whole,
                "{format} from the gallery differs from the file with every layer on the GPU"
            );
        }
    }
    drop(s);
    procraft_core::set_resident_budget(A4.ram / 4);
    if std::env::var_os("BIG_ARTWORK_DIR").is_none() {
        let _ = std::fs::remove_dir_all(&root);
    }
    let errors = gpu.errors();
    assert_eq!(errors, 0, "GPU validation errors");
    // opening keeps to the textures' budget and a few layers more; an export holds a few layers
    // whatever the artwork's number of them; from the gallery, the two together
    let layer = A4.layer_bytes();
    let opening = A4.ram / 4 + OPENING_MORE * layer;
    for (what, p) in &peaks {
        let most = match what.as_str() {
            "opened" => opening,
            w if w.ends_with("from the gallery") => opening + EXPORTING * layer,
            _ => EXPORTING * layer,
        };
        assert!(p.total() <= most, "{what}: {:.0} MB at the fullest, past {:.0} MB", mb(p.total()), mb(most));
    }
}

/// A3 at 400 dpi of 7 layers opens and shares within what a Galaxy Tab S7 of 6 GB leaves the app
/// (0.0.7 was stopped for memory doing either): the heap and the GPU together at their fullest
/// through opening it, showing it and sharing it in each format from the canvas, and through
/// sharing it in each format from the gallery, within [`A3_MOST`].
#[test]
fn an_a3_artwork_opens_and_shares_within_a_6_gb_tablet() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(gpu) = measured_gpu() else { return };
    let (root, art, out) = prepared(&A3);
    let art_path = art.to_string_lossy().into_owned();
    let mut peaks = Vec::new();
    // opened and shown, as tapping it in the gallery does, then shared in each format: what the
    // session holds at its fullest, the open canvas and each share together
    let t = Instant::now();
    let (_, canvas) = measure(&gpu, || {
        let s = CanvasSession::open(art_path.clone()).unwrap();
        s.use_gpu(gpu.clone());
        assert!(s.canvas_made());
        show(&A3, &s, &gpu);
        for format in FORMATS {
            let _ = std::fs::remove_file(export(&s, format, &out));
        }
    });
    line(&A3, &format!("opened and shared ({:.1} s)", t.elapsed().as_secs_f64()), canvas);
    peaks.push(("opened, shown and shared from the canvas".to_owned(), canvas));
    // shared from the gallery: opened only to be exported
    for format in FORMATS {
        let t = Instant::now();
        let (_, p) = measure(&gpu, || {
            let s = CanvasSession::open(art_path.clone()).unwrap();
            assert!(s.prepare_offscreen());
            let _ = std::fs::remove_file(export(&s, format, &out));
        });
        line(&A3, &format!("gallery {format} ({:.1} s)", t.elapsed().as_secs_f64()), p);
        peaks.push((format!("{format} from the gallery"), p));
    }
    if std::env::var_os("BIG_ARTWORK_DIR").is_none() {
        let _ = std::fs::remove_dir_all(&root);
    }
    assert_eq!(gpu.errors(), 0, "GPU validation errors");
    for (what, p) in &peaks {
        assert!(p.total() <= A3_MOST, "{what}: {:.0} MB at the fullest, past {:.0} MB", mb(p.total()), mb(A3_MOST));
    }
}
