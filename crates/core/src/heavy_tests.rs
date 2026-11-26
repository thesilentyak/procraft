//! What each library brush costs a frame through a long stroke, at a few sizes: the stamps each
//! frame lays, its CPU and GPU time, the stroke's start and its lift, and probes of where a heavy
//! brush's frames and ends go. Benches, run by hand with `--ignored --nocapture` (on a tablet too,
//! built with cargo-ndk). A tablet's GPU waited for after each frame slows its clocks between them
//! (a big brush's frame took three times as long so): its frame rate is measured with the GPU kept
//! busy, as the app keeps it ([`heavy_pipelined`]).

use std::io::Write;
use std::time::Instant;

use procraft_engine::view::View;
use procraft_gpu::wgpu;

use crate::tests::session;
use crate::{PenSample, PointerKind};

/// Renders what is queued and reads the composite.
fn composite(s: &crate::CanvasSession) -> Vec<u8> {
    let mut guard = s.shared.lock();
    let st = &mut *guard;
    let (canvas, gpu) = (st.canvas.as_mut().unwrap(), st.gpu.as_ref().unwrap());
    canvas.render(gpu, &mut st.doc, None);
    canvas.read_composite(gpu).unwrap()
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// The `k`th sample of the stroke (a pen at 240 Hz): back and forth across the canvas fitted in a
/// 2560 × 1600 window, a slow wave up and down, pressure rising and falling.
fn point(k: usize, width: f32, height: f32) -> PenSample {
    let side = height;
    let left = (width - side) / 2.0 + 80.0;
    let span = side - 160.0;
    let along = k as f32 * 6.0;
    let line = (along / span) as usize;
    let into = along % span;
    let x = if line.is_multiple_of(2) { left + into } else { left + span - into };
    let y = 160.0 + (line % 5) as f32 * 260.0 + (along * 0.01).sin() * 90.0;
    PenSample {
        x,
        y,
        pressure: 0.35 + 0.6 * (k as f32 * 0.02).sin().abs(),
        tilt: 0.0,
        orientation: 0.0,
        time_ms: k as f64 * 4.1667,
        roll: 0.0,
    }
}

/// Every library brush (or the sets `HEAVY_SETS`, the brushes `HEAVY_BRUSHES` names, `,` between)
/// drawn in a stroke `HEAVY_FRAMES` frames long (two pen samples a frame) at the size sliders
/// `HEAVY_SIZES`, a frame drawn into a 2560 × 1600 window and the GPU waited for after each. Writes
/// a row a stroke to `HEAVY_OUT` and a row a frame to `HEAVY_FRAMES_OUT`.
#[test]
#[ignore]
fn heavy_brush_frames() {
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
    let frames: usize = env("HEAVY_FRAMES", "240").parse().unwrap_or(240);
    let sizes: Vec<f32> = env("HEAVY_SIZES", "0.1,0.5,1.0").split(',').filter_map(|s| s.parse().ok()).collect();
    let sets = env("HEAVY_SETS", "");
    let names = env("HEAVY_BRUSHES", "");
    let side: u32 = env("HEAVY_CANVAS", "2048").parse().unwrap_or(2048);
    let aside = env("HEAVY_ASIDE", "1") == "1";
    let (width, height) = (2560u32, 1600u32);
    let mut out = std::fs::File::create(env("HEAVY_OUT", "heavy-strokes.tsv")).unwrap();
    let mut per_frame = std::fs::File::create(env("HEAVY_FRAMES_OUT", "heavy-frames.tsv")).unwrap();
    writeln!(
        out,
        "set\tbrush\tsize\tdiameter\tspacing\tcount\tdual\tin_place\tshape_side\tgrain_side\tstabilisation\tmotion\tstreamline\tselect_ms\tbegin_ms\tfirst_cpu\tfirst_gpu\tpictures_ms\tframes\tstamps_med\tstamps_max\tstamps_all\tadd_med\tadd_max\tcpu_med\tcpu_p95\tcpu_max\tgpu_med\tgpu_p95\tgpu_max\tframe_med\tframe_p95\tframe_max\tend_ms\tlift_cpu\tlift_gpu\tlift_frames\tlift_total\tlift_worst"
    )
    .unwrap();
    writeln!(per_frame, "set\tbrush\tsize\tframe\tadd\tstamps\tcpu\tgpu").unwrap();
    if aside {
        crate::big_pictures::ASIDE.with(|a| a.set(true));
    }
    let Some(s) = session(side, width) else { return };
    s.resize_window(width, height);
    s.set_display_scale(2.0);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = {
        let mut st = s.shared.lock();
        st.view = Some(View::fit((side, side), (width, height)));
        let gpu = st.gpu.as_ref().unwrap();
        let info = gpu.adapter.get_info();
        println!("{} ({:?}), window {width}x{height}, canvas {side} square", info.name, info.backend);
        gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("window"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    };
    let view = target.create_view(&Default::default());
    let started = Instant::now();
    let gpu = s.shared.lock().gpu.clone().unwrap();
    // a frame: its CPU time (recorded and sent), then the GPU's (waited for)
    let frame = || {
        let t = Instant::now();
        let more = {
            let mut st = s.shared.lock();
            st.draw_frame(&view, format, (width, height), procraft_engine::profile::Profile::Srgb, started);
            st.needs_frame || st.canvas.as_ref().is_some_and(|c| c.lays_over_frames())
        };
        let cpu = ms(t);
        let t = Instant::now();
        gpu.flush();
        (cpu, ms(t), more)
    };
    let queued = || s.shared.lock().canvas.as_ref().map_or(0, |c| c.stamps_queued());
    let at = |v: &[f64], q: f64| {
        let mut v = v.to_vec();
        v.sort_by(f64::total_cmp);
        v.get(((v.len().max(1) - 1) as f64 * q).round() as usize).copied().unwrap_or(0.0)
    };
    let most = |v: &[f64]| v.iter().copied().fold(0.0, f64::max);
    // paint to mix into
    let library = s.brush_library();
    if let Some((set, index)) = library
        .iter()
        .enumerate()
        .find_map(|(i, set)| set.brushes.iter().position(|b| b.name == "Soft Airbrush").map(|j| (i as u32, j as u32)))
    {
        s.select_brush(set, index);
        s.set_brush_size(1.0);
        for k in 0..4 {
            s.set_color(k as f32 / 3.0, 0.4, 1.0 - k as f32 / 3.0);
            s.stroke_begin(PointerKind::Stylus, vec![point(k * 300, width as f32, height as f32)]);
            s.stroke_add((1..80).map(|i| point(k * 300 + i * 3, width as f32, height as f32)).collect());
            s.stroke_end(vec![]);
            frame();
        }
    }
    s.set_color(0.8, 0.2, 0.1);
    for set in &library {
        if !sets.is_empty() && !sets.split(',').any(|n| n == set.name) {
            continue;
        }
        for b in &set.brushes {
            if !names.is_empty() && !names.split(',').any(|n| n == b.name) {
                continue;
            }
            let t = Instant::now();
            s.select_brush(b.set, b.index);
            let select = ms(t);
            // settings changed for the run (`HEAVY_SETTINGS`: key=value, `,` between)
            let settings = env("HEAVY_SETTINGS", "");
            if !settings.is_empty() {
                s.studio_begin();
                for kv in settings.split(',') {
                    if let Some((k, v)) = kv.split_once('=')
                        && let Ok(v) = v.parse::<f32>()
                    {
                        s.set_studio_setting(k.to_owned(), v);
                    }
                }
                s.studio_end(true);
            }
            let brush = s.shared.lock().brush(crate::ToolKind::Paint).clone();
            let style = procraft_gpu::StrokeStyle::for_brush(
                &brush,
                procraft_gpu::StrokeTool::Paint,
                [0.8, 0.2, 0.1],
                1.0,
                false,
            );
            let pictures = Instant::now();
            crate::big_pictures::wait_for_big_pictures(std::time::Duration::from_secs(60));
            let pictures = ms(pictures);
            let shape_side = brush
                .shape_file_now()
                .map(|f| f.full.side)
                .or_else(|| crate::big_pictures::shape_of(&brush).map(|b| b.side))
                .unwrap_or(0);
            let grain_side = brush
                .grain_file_now()
                .and_then(|f| f.full.as_ref().map(|g| g.side))
                .or_else(|| crate::big_pictures::grain_of(&brush).map(|b| b.side))
                .unwrap_or(0);
            for (n, &size) in sizes.iter().enumerate() {
                s.set_brush_size(size);
                let diameter = brush.diameter(size);
                let t = Instant::now();
                s.stroke_begin(PointerKind::Stylus, vec![point(0, width as f32, height as f32)]);
                let begin = ms(t);
                let (first_cpu, first_gpu, _) = frame();
                let mut last = queued();
                let (mut adds, mut cpus, mut gpus, mut whole, mut stamps) =
                    (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
                for f in 1..=frames {
                    let t = Instant::now();
                    s.stroke_add(vec![
                        point(2 * f - 1, width as f32, height as f32),
                        point(2 * f, width as f32, height as f32),
                    ]);
                    let add = ms(t);
                    let (cpu, gpu_ms, _) = frame();
                    let now = queued();
                    let laid = now.saturating_sub(last) as f64;
                    last = now;
                    writeln!(
                        per_frame,
                        "{}\t{}\t{size}\t{f}\t{add:.3}\t{laid}\t{cpu:.3}\t{gpu_ms:.3}",
                        set.name, b.name
                    )
                    .unwrap();
                    adds.push(add);
                    cpus.push(cpu);
                    gpus.push(gpu_ms);
                    whole.push(add + cpu + gpu_ms);
                    stamps.push(laid);
                }
                let t = Instant::now();
                s.stroke_end(vec![]);
                let end = ms(t);
                let (mut lift_cpu, mut lift_gpu, mut lift_frames, mut lift_worst) = (0.0, 0.0, 0usize, 0.0f64);
                loop {
                    let (cpu, gpu_ms, more) = frame();
                    lift_cpu += cpu;
                    lift_gpu += gpu_ms;
                    lift_frames += 1;
                    lift_worst = lift_worst.max(cpu + gpu_ms);
                    if !more || lift_frames >= 400 {
                        break;
                    }
                }
                writeln!(
                    per_frame,
                    "{}\t{}\t{size}\tlift\t{end:.3}\t{}\t{lift_cpu:.3}\t{lift_gpu:.3}",
                    set.name,
                    b.name,
                    queued().saturating_sub(last)
                )
                .unwrap();
                let lift_total = end + lift_cpu + lift_gpu;
                let row = format!(
                    "{}\t{}\t{size}\t{diameter:.1}\t{:.4}\t{}\t{}\t{}\t{shape_side}\t{grain_side}\t{:.2}\t{:.2}\t{:.2}\t{:.2}\t{begin:.2}\t{first_cpu:.2}\t{first_gpu:.2}\t{:.1}\t{frames}\t{}\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{:.3}\t{end:.2}\t{lift_cpu:.2}\t{lift_gpu:.2}\t{lift_frames}\t{lift_total:.2}\t{lift_worst:.2}",
                    set.name,
                    b.name,
                    brush.spacing,
                    brush.shape.count,
                    u8::from(brush.secondary.is_some()),
                    u8::from(style.in_place()),
                    brush.setting("stabilization"),
                    brush.setting("motion_filtering"),
                    brush.streamline,
                    if n == 0 { select } else { 0.0 },
                    if n == 0 { pictures } else { 0.0 },
                    at(&stamps, 0.5),
                    most(&stamps),
                    stamps.iter().sum::<f64>(),
                    at(&adds, 0.5),
                    most(&adds),
                    at(&cpus, 0.5),
                    at(&cpus, 0.95),
                    most(&cpus),
                    at(&gpus, 0.5),
                    at(&gpus, 0.95),
                    most(&gpus),
                    at(&whole, 0.5),
                    at(&whole, 0.95),
                    most(&whole),
                );
                writeln!(out, "{row}").unwrap();
                println!("{row}");
                s.undo();
                frame();
            }
        }
    }
}

/// Why a stroke's end lays what it lays: for the brushes `HEAVY_BRUSHES` names at `HEAVY_SIZES`,
/// what each part has laid when the pen lifts and where its end would lay from.
#[test]
#[ignore]
fn heavy_lift_probe() {
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
    let frames: usize = env("HEAVY_FRAMES", "240").parse().unwrap_or(240);
    let sizes: Vec<f32> = env("HEAVY_SIZES", "0.1,1.0").split(',').filter_map(|s| s.parse().ok()).collect();
    let names = env("HEAVY_BRUSHES", "Verbena,Elfin Saddle,Easterly,Hamilton,Torrent");
    let (width, height) = (2560u32, 1600u32);
    let Some(s) = session(2048, width) else { return };
    s.resize_window(width, height);
    {
        let mut st = s.shared.lock();
        st.view = Some(View::fit((2048, 2048), (width, height)));
    }
    let library = s.brush_library();
    for name in names.split(',') {
        let Some(b) = library.iter().flat_map(|set| set.brushes.iter()).find(|b| b.name == name) else {
            println!("no {name}");
            continue;
        };
        s.select_brush(b.set, b.index);
        for &size in &sizes {
            s.set_brush_size(size);
            s.stroke_begin(PointerKind::Stylus, vec![point(0, width as f32, height as f32)]);
            for f in 1..=frames {
                s.stroke_add(vec![
                    point(2 * f - 1, width as f32, height as f32),
                    point(2 * f, width as f32, height as f32),
                ]);
                let mut st = s.shared.lock();
                let st = &mut *st;
                if let (Some(c), Some(g)) = (st.canvas.as_mut(), st.gpu.as_ref()) {
                    c.render(g, &mut st.doc, None);
                }
            }
            {
                let st = s.shared.lock();
                let c = st.canvas.as_ref().unwrap();
                let first = st.stroke.clone().map(|mut b| {
                    let n = c.laid_count();
                    let placed = b.stamp_count();
                    (n, placed, n.and_then(|n| b.finish_changed(n).map(|(from, rest)| (from, rest.len()))))
                });
                let second = st.second.clone().map(|mut b| {
                    let n = c.laid_second_count();
                    (n, n.and_then(|n| b.finish_changed(n).map(|(from, rest)| (from, rest.len()))))
                });
                println!(
                    "{name} {size}: first laid {:?} placed {:?} -> {:?}; second laid,end {:?}; keeps_laid {}",
                    first.map(|f| f.0),
                    first.map(|f| f.1),
                    first.map(|f| f.2),
                    second,
                    c.keeps_laid()
                );
            }
            let before = s.shared.lock().canvas.as_ref().map_or(0, |c| c.stamps_queued());
            s.stroke_end(vec![]);
            let after = s.shared.lock().canvas.as_ref().map_or(0, |c| c.stamps_queued());
            println!("   end queued {}", after - before);
            s.undo();
        }
    }
}

/// Where a big brush's frame goes: frames with new samples drawn whole, frames with none, and
/// frames with new samples whose canvas work alone is drawn (no window), the GPU waited for after
/// each; medians, for the brushes `HEAVY_BRUSHES` names at `HEAVY_SIZES`.
#[test]
#[ignore]
fn heavy_frame_parts() {
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
    let sizes: Vec<f32> = env("HEAVY_SIZES", "1.0").split(',').filter_map(|s| s.parse().ok()).collect();
    let names = env("HEAVY_BRUSHES", "Chroma,Severn,Holdencroft,Longstone");
    let side: u32 = env("HEAVY_CANVAS", "2048").parse().unwrap_or(2048);
    let (width, height) = (2560u32, 1600u32);
    let Some(s) = session(side, width) else { return };
    s.resize_window(width, height);
    s.set_display_scale(2.0);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = {
        let mut st = s.shared.lock();
        st.view = Some(View::fit((side, side), (width, height)));
        st.gpu.as_ref().unwrap().device.create_texture(&wgpu::TextureDescriptor {
            label: Some("window"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    };
    let view = target.create_view(&Default::default());
    let gpu = s.shared.lock().gpu.clone().unwrap();
    let started = Instant::now();
    let median = |v: &mut Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let library = s.brush_library();
    for name in names.split(',') {
        let Some(b) = library.iter().flat_map(|set| set.brushes.iter()).find(|b| b.name == name) else {
            println!("no {name}");
            continue;
        };
        s.select_brush(b.set, b.index);
        crate::big_pictures::wait_for_big_pictures(std::time::Duration::from_secs(60));
        for &size in &sizes {
            s.set_brush_size(size);
            s.stroke_begin(PointerKind::Stylus, vec![point(0, width as f32, height as f32)]);
            let (mut whole, mut idle, mut canvas_only, mut cpu_whole, mut cpu_canvas) =
                (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for f in 1..=120 {
                s.stroke_add(vec![
                    point(2 * f - 1, width as f32, height as f32),
                    point(2 * f, width as f32, height as f32),
                ]);
                let t = Instant::now();
                match f % 3 {
                    0 => {
                        s.shared.lock().draw_frame(
                            &view,
                            format,
                            (width, height),
                            procraft_engine::profile::Profile::Srgb,
                            started,
                        );
                        cpu_whole.push(ms(t));
                        gpu.flush();
                        whole.push(ms(t));
                        // and one with nothing new
                        let t = Instant::now();
                        s.shared.lock().needs_frame = true;
                        s.shared.lock().draw_frame(
                            &view,
                            format,
                            (width, height),
                            procraft_engine::profile::Profile::Srgb,
                            started,
                        );
                        gpu.flush();
                        idle.push(ms(t));
                    }
                    1 => {
                        {
                            let mut st = s.shared.lock();
                            let st = &mut *st;
                            st.draw_ahead();
                            if let (Some(c), Some(g)) = (st.canvas.as_mut(), st.gpu.as_ref()) {
                                c.render(g, &mut st.doc, None);
                            }
                        }
                        cpu_canvas.push(ms(t));
                        gpu.flush();
                        canvas_only.push(ms(t));
                    }
                    _ => {
                        s.shared.lock().draw_frame(
                            &view,
                            format,
                            (width, height),
                            procraft_engine::profile::Profile::Srgb,
                            started,
                        );
                        gpu.flush();
                    }
                }
            }
            s.stroke_end(vec![]);
            s.shared.lock().draw_frame(
                &view,
                format,
                (width, height),
                procraft_engine::profile::Profile::Srgb,
                started,
            );
            gpu.flush();
            println!(
                "{name} {size}: whole frame {:.2} ms (cpu {:.2}), nothing new {:.2}, canvas work alone {:.2} (cpu {:.2})",
                median(&mut whole),
                median(&mut cpu_whole),
                median(&mut idle),
                median(&mut canvas_only),
                median(&mut cpu_canvas)
            );
            s.undo();
            s.shared.lock().draw_frame(
                &view,
                format,
                (width, height),
                procraft_engine::profile::Profile::Srgb,
                started,
            );
            gpu.flush();
        }
    }
}

/// What the first stroke after choosing a brush waits for: each library brush (or `HEAVY_BRUSHES`)
/// chosen, its pictures drawn aside as the app draws them, and the pen put down `HEAVY_AFTER_MS`
/// (0) later: how long the stroke's start took, and how long drawing its pictures took in all.
#[test]
#[ignore]
fn heavy_first_stroke() {
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
    let names = env("HEAVY_BRUSHES", "");
    let after: u64 = env("HEAVY_AFTER_MS", "0").parse().unwrap_or(0);
    // big pictures made aside, as the app makes them (a stroke meanwhile reads those brushes hold)
    crate::big_pictures::ASIDE.with(|a| a.set(true));
    let Some(s) = session(2048, 2048) else { return };
    let library = s.brush_library();
    let mut rows = Vec::new();
    for b in library.iter().flat_map(|set| set.brushes.iter()) {
        if !names.is_empty() && !names.split(',').any(|n| n == b.name) {
            continue;
        }
        let brush = s.shared.lock().library[b.set as usize].brushes[b.index as usize].clone();
        let pictures: Vec<procraft_engine::brush::Pixels> = std::iter::once(&brush)
            .chain(brush.secondary.as_deref())
            .flat_map(|b| [&b.shape.source, &b.grain.source])
            .flatten()
            .map(|s| s.pixels.clone())
            .collect();
        let undrawn = pictures.iter().filter(|p| !p.is_drawn()).count();
        let chosen = Instant::now();
        s.select_brush(b.set, b.index);
        std::thread::sleep(std::time::Duration::from_millis(after));
        let t = Instant::now();
        s.stroke_begin(PointerKind::Stylus, vec![point(0, 2048.0, 2048.0)]);
        let begin = ms(t);
        s.stroke_cancel();
        // how long its pictures took to draw from its choosing
        while !pictures.iter().all(procraft_engine::brush::Pixels::is_drawn) && chosen.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
        let drawn = ms(chosen);
        rows.push((begin, drawn, undrawn, b.name.clone()));
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    let waited: Vec<f64> = rows.iter().map(|r| r.0).collect();
    let over = |ms: f64| waited.iter().filter(|&&w| w > ms).count();
    println!(
        "{} brushes, pen down {after} ms after choosing: start waited over 16 ms {}, over 50 ms {}, over 100 ms {}",
        rows.len(),
        over(16.0),
        over(50.0),
        over(100.0)
    );
    for (begin, drawn, undrawn, name) in rows.iter().take(15) {
        println!("  {begin:7.1} ms  {name} ({undrawn} pictures to draw, all drawn {drawn:.0} ms after choosing)");
    }
}

/// Every library brush that lays a part by its rule (or `HEAVY_BRUSHES`), drawn on a 768 square
/// canvas at a few sizes: the picture mid-stroke (its tip showing) and the layer once it ends,
/// written to `HEAVY_PIXELS_OUT` to set beside another build's.
#[test]
#[ignore]
fn heavy_rule_pixels() {
    use procraft_engine::brush::Rendering;
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
    let names = env("HEAVY_BRUSHES", "");
    let side = 768u32;
    let Some(s) = session(side, side) else { return };
    let mut out = std::fs::File::create(env("HEAVY_PIXELS_OUT", "rule-pixels.bin")).unwrap();
    let library = s.brush_library();
    let by_rule = |b: &procraft_engine::brush::Brush| matches!(b.rendering, Rendering::Gradual | Rendering::Capped);
    let point = |k: usize| PenSample {
        x: 40.0 + (k % 240) as f32 * 2.9,
        y: 120.0 + (k / 240) as f32 * 260.0 + (k as f32 * 0.03).sin() * 90.0,
        pressure: 0.3 + 0.65 * (k as f32 * 0.021).sin().abs(),
        tilt: 0.0,
        orientation: 0.0,
        time_ms: k as f64 * 4.17,
        roll: 0.0,
    };
    let mut drawn = 0;
    for b in library.iter().flat_map(|set| set.brushes.iter()) {
        let brush = s.shared.lock().library[b.set as usize].brushes[b.index as usize].clone();
        let wanted = if names.is_empty() {
            (by_rule(&brush) || brush.secondary.as_deref().is_some_and(by_rule)) && !brush.mixes_wet()
        } else {
            names.split(',').any(|n| n == b.name)
        };
        if !wanted {
            continue;
        }
        s.select_brush(b.set, b.index);
        for size in [0.25f32, 0.7, 1.0] {
            s.set_brush_size(size);
            s.set_color(0.2, 0.45, 0.8);
            s.stroke_begin(PointerKind::Stylus, vec![point(0)]);
            let mut mid = Vec::new();
            for e in 1..300 {
                s.stroke_add(vec![point(2 * e - 1), point(2 * e)]);
                if e % 3 == 0 {
                    let pic = composite(&s);
                    if e == 150 {
                        mid = pic;
                    }
                }
            }
            s.stroke_end(vec![]);
            composite(&s);
            let current = s.layers().iter().find(|l| l.current).map_or(0, |l| l.id);
            let layer = s.shared.lock().layer_pixels(procraft_engine::document::LayerId(current)).unwrap();
            let head = format!("{}/{}/{size}\n", b.set, b.name);
            out.write_all(head.as_bytes()).unwrap();
            out.write_all(&(mid.len() as u64).to_le_bytes()).unwrap();
            out.write_all(&mid).unwrap();
            out.write_all(&(layer.len() as u64).to_le_bytes()).unwrap();
            out.write_all(&layer).unwrap();
            s.undo();
            composite(&s);
            drawn += 1;
        }
    }
    println!("{drawn} strokes written");
}

/// A stroke's frames one after another as fast as they go, the GPU waited for only at the end (so
/// that it is kept busy, as the app keeps it, its clocks up): milliseconds a frame, for the
/// brushes `HEAVY_BRUSHES` names at `HEAVY_SIZES`, `HEAVY_ROUNDS` times each.
#[test]
#[ignore]
fn heavy_pipelined() {
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
    let sizes: Vec<f32> = env("HEAVY_SIZES", "1.0").split(',').filter_map(|s| s.parse().ok()).collect();
    let names = env("HEAVY_BRUSHES", "Chroma");
    let rounds: usize = env("HEAVY_ROUNDS", "2").parse().unwrap_or(2);
    let (width, height) = (2560u32, 1600u32);
    let side = 2048;
    let Some(s) = session(side, width) else { return };
    s.resize_window(width, height);
    s.set_display_scale(2.0);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = {
        let mut st = s.shared.lock();
        st.view = Some(View::fit((side, side), (width, height)));
        st.gpu.as_ref().unwrap().device.create_texture(&wgpu::TextureDescriptor {
            label: Some("window"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    };
    let view = target.create_view(&Default::default());
    let gpu = s.shared.lock().gpu.clone().unwrap();
    let started = Instant::now();
    let library = s.brush_library();
    for _ in 0..rounds {
        for name in names.split(',') {
            let Some(b) = library.iter().flat_map(|set| set.brushes.iter()).find(|b| b.name == name) else { continue };
            s.select_brush(b.set, b.index);
            for &size in &sizes {
                s.set_brush_size(size);
                s.stroke_begin(PointerKind::Stylus, vec![point(0, width as f32, height as f32)]);
                let frame = |f: usize| {
                    s.stroke_add(vec![
                        point(2 * f - 1, width as f32, height as f32),
                        point(2 * f, width as f32, height as f32),
                    ]);
                    s.shared.lock().draw_frame(
                        &view,
                        format,
                        (width, height),
                        procraft_engine::profile::Profile::Srgb,
                        started,
                    );
                };
                for f in 1..=20 {
                    frame(f);
                }
                gpu.flush();
                let t = Instant::now();
                for f in 21..=140 {
                    frame(f);
                    // (a frame or two ahead at most, as a window's images allow)
                    if f.is_multiple_of(2) {
                        gpu.flush();
                    }
                }
                gpu.flush();
                let each = ms(t) / 120.0;
                s.stroke_end(vec![]);
                s.shared.lock().draw_frame(
                    &view,
                    format,
                    (width, height),
                    procraft_engine::profile::Profile::Srgb,
                    started,
                );
                gpu.flush();
                s.undo();
                s.shared.lock().draw_frame(
                    &view,
                    format,
                    (width, height),
                    procraft_engine::profile::Profile::Srgb,
                    started,
                );
                gpu.flush();
                println!("{name} {size}: {each:.2} ms a frame");
            }
        }
    }
}

/// The first frames of a few strokes (`HEAVY_STROKES`, 4) of each brush `HEAVY_BRUSHES` (Verbena)
/// at the size sliders `HEAVY_SIZES` (1.0), on a `HEAVY_W` × `HEAVY_H` canvas (2560 × 1600) shown
/// whole in a window its size: the pen put down and held still `HEAVY_STILL` frames (0), then
/// moved `HEAVY_STEP` pixels a sample (6), `HEAVY_PER_FRAME` samples a frame (2), as a stylus or
/// (`HEAVY_POINTER` mouse) a mouse at full pressure. Each of a stroke's first `HEAVY_FIRST` frames
/// (12): what it took to record and send, then for the GPU (waited for), and the slowest.
#[test]
#[ignore]
fn heavy_stroke_starts() {
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
    let names = env("HEAVY_BRUSHES", "Verbena");
    let sizes: Vec<f32> = env("HEAVY_SIZES", "1.0").split(',').filter_map(|s| s.parse().ok()).collect();
    let strokes: usize = env("HEAVY_STROKES", "4").parse().unwrap_or(4);
    let (width, height): (u32, u32) =
        (env("HEAVY_W", "2560").parse().unwrap_or(2560), env("HEAVY_H", "1600").parse().unwrap_or(1600));
    let still: usize = env("HEAVY_STILL", "0").parse().unwrap_or(0);
    let step: f32 = env("HEAVY_STEP", "6").parse().unwrap_or(6.0);
    let per_frame: usize = env("HEAVY_PER_FRAME", "2").parse().unwrap_or(2);
    let first: usize = env("HEAVY_FIRST", "12").parse().unwrap_or(12);
    let mouse = env("HEAVY_POINTER", "stylus") == "mouse";
    crate::big_pictures::ASIDE.with(|a| a.set(true));
    let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
    let s = crate::CanvasSession::new(width, height);
    {
        let mut st = s.shared.lock();
        st.window_size = (width, height);
        st.view = Some(View::fit((width, height), (width, height)));
        st.gpu = Some(gpu);
        st.ensure_canvas();
    }
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = s.shared.lock().gpu.as_ref().unwrap().device.create_texture(&wgpu::TextureDescriptor {
        label: Some("window"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let gpu = s.shared.lock().gpu.clone().unwrap();
    let started = Instant::now();
    let frame = || {
        let t = Instant::now();
        s.shared.lock().draw_frame(&view, format, (width, height), procraft_engine::profile::Profile::Srgb, started);
        let cpu = ms(t);
        let t = Instant::now();
        gpu.flush();
        (cpu, ms(t))
    };
    let pointer = if mouse { PointerKind::Mouse } else { PointerKind::Stylus };
    let library = s.brush_library();
    let info = gpu.adapter.get_info();
    println!(
        "{} ({:?}), canvas {width}x{height}, still {still}, step {step}, {per_frame} a frame, {pointer:?}",
        info.name, info.backend
    );
    // (`HEAVY_BETWEEN`: a brush chosen before each stroke, and the stroke's own chosen again after
    // it; `HEAVY_WAIT_MS`: frames drawn this long after each choice, as the window draws them)
    let between = env("HEAVY_BETWEEN", "");
    let wait_ms: u64 = env("HEAVY_WAIT_MS", "0").parse().unwrap_or(0);
    let find = |name: &str| library.iter().flat_map(|set| set.brushes.iter()).find(|b| b.name == name).cloned();
    let idle = |for_ms: u64| {
        let t = Instant::now();
        frame();
        while t.elapsed().as_millis() < u128::from(for_ms) {
            frame();
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    };
    idle(200);
    for name in names.split(',') {
        let Some(b) = find(name) else { continue };
        s.select_brush(b.set, b.index);
        crate::big_pictures::wait_for_big_pictures(std::time::Duration::from_secs(60));
        idle(wait_ms);
        for &size in &sizes {
            s.set_brush_size(size);
            for n in 0..strokes {
                if let Some(other) = find(&between) {
                    s.select_brush(other.set, other.index);
                    idle(wait_ms);
                    s.select_brush(b.set, b.index);
                    s.set_brush_size(size);
                    idle(wait_ms);
                }
                let y = height as f32 * (0.25 + 0.5 * n as f32 / strokes.max(1) as f32);
                let mut k = 0usize;
                let sample = |k: usize| PenSample {
                    x: width as f32 * 0.2 + k as f32 * step,
                    y: y + (k as f32 * 0.05).sin() * 20.0,
                    pressure: if mouse { 1.0 } else { 0.35 + 0.6 * (k as f32 * 0.02).sin().abs() },
                    tilt: 0.0,
                    orientation: 0.0,
                    time_ms: k as f64 * 1000.0 / (60.0 * per_frame as f64) + n as f64 * 10_000.0,
                    roll: 0.0,
                };
                let t = Instant::now();
                s.stroke_begin(pointer, vec![sample(0)]);
                let begin = ms(t);
                let mut times = Vec::new();
                for f in 0..first.max(1) + 40 {
                    let t = Instant::now();
                    if f >= still {
                        let samples: Vec<PenSample> = (0..per_frame)
                            .map(|_| {
                                k += 1;
                                sample(k)
                            })
                            .collect();
                        s.stroke_add(samples);
                    }
                    let add = ms(t);
                    let (cpu, gpu_ms) = frame();
                    times.push((add, cpu, gpu_ms));
                }
                s.stroke_end(vec![]);
                for _ in 0..6 {
                    frame();
                }
                let (worst, at) = times
                    .iter()
                    .enumerate()
                    .take(first)
                    .map(|(i, (a, c, g))| (a + c + g, i))
                    .fold((0.0, 0), |m, x| if x.0 > m.0 { x } else { m });
                let rest = times.iter().skip(first).map(|(a, c, g)| a + c + g).fold(0.0, f64::max);
                println!(
                    "{name} {size} stroke {}: begin {begin:.1} ms; first frames (add+cpu+gpu) {}; worst {worst:.1} ms at frame {}; later frames at most {rest:.1}",
                    n + 1,
                    times.iter().take(first).map(|(a, c, g)| format!("{:.1}", a + c + g)).collect::<Vec<_>>().join(" "),
                    at + 1,
                );
                if env("HEAVY_PARTS", "0") == "1" {
                    println!(
                        "    parts {}",
                        times
                            .iter()
                            .take(first)
                            .map(|(a, c, g)| format!("{a:.1}/{c:.1}/{g:.1}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    );
                }
            }
        }
    }
}

/// A session with one brush, as a long one in the app: `HEAVY_BRUSHES` (Verbena) at the size slider
/// `HEAVY_SIZE` (1.0), `HEAVY_STROKES` (40) strokes of `HEAVY_STROKE_MS` (900) ms each, back and forth
/// over a `HEAVY_W` × `HEAVY_H` canvas (2560 × 1600) shown whole, two pen samples a frame, the GPU
/// waited for after each frame. Per stroke: its frames' median and slowest, its lift, and what the
/// canvas holds (its textures, the undo history's, and the GPU allocator's where it tells).
#[test]
#[ignore]
fn heavy_session() {
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
    let name = env("HEAVY_BRUSHES", "Verbena");
    let size: f32 = env("HEAVY_SIZE", "1.0").parse().unwrap_or(1.0);
    let strokes: usize = env("HEAVY_STROKES", "40").parse().unwrap_or(40);
    let stroke_ms: f64 = env("HEAVY_STROKE_MS", "900").parse().unwrap_or(900.0);
    let (width, height): (u32, u32) =
        (env("HEAVY_W", "2560").parse().unwrap_or(2560), env("HEAVY_H", "1600").parse().unwrap_or(1600));
    crate::big_pictures::ASIDE.with(|a| a.set(true));
    let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
    let s = crate::CanvasSession::new(width, height);
    {
        let mut st = s.shared.lock();
        st.window_size = (width, height);
        st.view = Some(View::fit((width, height), (width, height)));
        st.gpu = Some(gpu);
        st.ensure_canvas();
    }
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = s.shared.lock().gpu.as_ref().unwrap().device.create_texture(&wgpu::TextureDescriptor {
        label: Some("window"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let gpu = s.shared.lock().gpu.clone().unwrap();
    let started = Instant::now();
    let frame = || {
        let t = Instant::now();
        let more = {
            let mut st = s.shared.lock();
            st.draw_frame(&view, format, (width, height), procraft_engine::profile::Profile::Srgb, started);
            st.needs_frame || st.canvas.as_ref().is_some_and(|c| c.lays_over_frames())
        };
        gpu.flush();
        (ms(t), more)
    };
    let library = s.brush_library();
    let Some(b) = library.iter().flat_map(|set| set.brushes.iter()).find(|b| b.name == name) else { return };
    s.select_brush(b.set, b.index);
    crate::big_pictures::wait_for_big_pictures(std::time::Duration::from_secs(60));
    s.set_brush_size(size);
    let info = gpu.adapter.get_info();
    println!(
        "{} ({:?}), canvas {width}x{height}, {name} at {size}, {strokes} strokes of {stroke_ms} ms",
        info.name, info.backend
    );
    let mb = |b: u64| b as f64 / f64::from(1u32 << 20);
    let samples = (stroke_ms / 1000.0 * 240.0) as usize;
    let mut t_ms = 0.0f64;
    for n in 0..strokes {
        // back and forth across the canvas, a band lower each stroke
        let y0 = height as f32 * (0.15 + 0.1 * (n % 8) as f32);
        let point = |k: usize, t_ms: f64| {
            let along = k as f32 / samples.max(1) as f32;
            let x = width as f32 * if n.is_multiple_of(2) { 0.1 + 0.8 * along } else { 0.9 - 0.8 * along };
            PenSample {
                x,
                y: y0 + (along * 9.0).sin() * height as f32 * 0.05,
                pressure: 0.5 + 0.45 * (along * 7.0).sin().abs(),
                tilt: 0.0,
                orientation: 0.0,
                time_ms: t_ms + k as f64 * 1000.0 / 240.0,
                roll: 0.0,
            }
        };
        let began = Instant::now();
        s.stroke_begin(PointerKind::Stylus, vec![point(0, t_ms)]);
        let mut frames = Vec::new();
        let mut k = 0;
        while k + 2 < samples {
            s.stroke_add(vec![point(k + 1, t_ms), point(k + 2, t_ms)]);
            k += 2;
            frames.push(frame().0);
        }
        let lifted = Instant::now();
        s.stroke_end(vec![point(samples, t_ms)]);
        let mut lift = Vec::new();
        for _ in 0..30 {
            let (took, more) = frame();
            lift.push(took);
            if !more {
                break;
            }
        }
        let lift_ms = ms(lifted);
        t_ms += stroke_ms + 400.0;
        frames.sort_by(f64::total_cmp);
        let (textures, history, packed, count) = {
            let st = s.shared.lock();
            let c = st.canvas.as_ref().unwrap();
            (c.texture_bytes(), c.history_bytes(), c.history_packed_bytes(), c.textures().len())
        };
        let report = gpu.device.generate_allocator_report().map(|r| (r.total_allocated_bytes, r.total_reserved_bytes));
        println!(
            "stroke {:2}: frames med {:5.1} max {:6.1} ms, lift {:6.1} ms in {} frames (worst {:5.1}); {:5.0} ms in all | textures {:6.0} MB ({count}), undo {:5.0} MB, packed {:4.0} MB{}",
            n + 1,
            frames.get(frames.len() / 2).copied().unwrap_or(0.0),
            frames.last().copied().unwrap_or(0.0),
            lift_ms,
            lift.len(),
            lift.iter().copied().fold(0.0, f64::max),
            ms(began),
            mb(textures),
            mb(history),
            mb(packed),
            report.map_or(String::new(), |(a, r)| format!(", GPU allocated {:.0} MB, reserved {:.0} MB", mb(a), mb(r))),
        );
    }
}

/// As [`heavy_session`], the frames drawn as the app draws them: on a thread of their own every
/// 8 ms (the GPU waited for after each, as a window's one image in flight is), the pen's samples
/// coming in real time (`HEAVY_RATE`, 120 a second) on another. Per stroke: its slowest frames (from
/// the draw's start to its GPU work done) and how many took over 100 ms, as the app logs them.
#[test]
#[ignore]
fn heavy_realtime() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    let env = |name: &str, default: &str| std::env::var(name).unwrap_or_else(|_| default.to_owned());
    let name = env("HEAVY_BRUSHES", "Verbena");
    let size: f32 = env("HEAVY_SIZE", "1.0").parse().unwrap_or(1.0);
    let strokes: usize = env("HEAVY_STROKES", "40").parse().unwrap_or(40);
    let stroke_ms: f64 = env("HEAVY_STROKE_MS", "900").parse().unwrap_or(900.0);
    let rate: f64 = env("HEAVY_RATE", "120").parse().unwrap_or(120.0);
    let (width, height): (u32, u32) =
        (env("HEAVY_W", "2560").parse().unwrap_or(2560), env("HEAVY_H", "1600").parse().unwrap_or(1600));
    crate::big_pictures::ASIDE.with(|a| a.set(true));
    let Ok(gpu) = procraft_gpu::Gpu::headless() else { return };
    let s = crate::CanvasSession::new(width, height);
    {
        let mut st = s.shared.lock();
        st.window_size = (width, height);
        st.view = Some(View::fit((width, height), (width, height)));
        st.gpu = Some(gpu);
        // (`HEAVY_P3`: the canvas in Display P3, as a Screen Size one is made)
        if env("HEAVY_P3", "0") == "1" {
            st.doc.assign_profile(procraft_engine::profile::Profile::DisplayP3, None);
        }
        st.ensure_canvas();
    }
    let library = s.brush_library();
    let Some(b) = library.iter().flat_map(|set| set.brushes.iter()).find(|b| b.name == name) else { return };
    s.select_brush(b.set, b.index);
    crate::big_pictures::wait_for_big_pictures(std::time::Duration::from_secs(60));
    s.set_brush_size(size);
    let stop = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let frames = {
        let (s, stop) = (s.clone(), stop.clone());
        std::thread::spawn(move || {
            let gpu = s.shared.lock().gpu.clone().unwrap();
            let format = wgpu::TextureFormat::Rgba8Unorm;
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("window"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let mut taken = Vec::new();
            while !stop.load(Ordering::Relaxed) {
                let queued = s.shared.lock().canvas.as_ref().map(|c| c.queued_work()).unwrap_or_default();
                let t = Instant::now();
                match s.shared.state.try_lock_for(Duration::from_millis(40)) {
                    Some(guard) => drop(guard),
                    None => {
                        println!("  frame waits for the canvas: {}", s.shared.holder.said());
                        drop(s.shared.lock());
                    }
                }
                let locked = ms(t);
                let d = Instant::now();
                s.draw(&view, format, (width, height), procraft_engine::profile::Profile::Srgb, started);
                let drew = ms(d);
                let f = Instant::now();
                gpu.flush();
                let flushed = ms(f);
                let at = t.duration_since(started).as_secs_f64() * 1e3;
                if ms(t) > 50.0 {
                    println!(
                        "  slow frame at {at:.0} ms: lock {locked:.1}, draw {drew:.1}, GPU {flushed:.1} | queued: {queued}"
                    );
                }
                taken.push((at, ms(t), flushed));
                std::thread::sleep(Duration::from_millis(8).saturating_sub(t.elapsed()));
            }
            taken
        })
    };
    let info = s.shared.lock().gpu.as_ref().map(|g| g.adapter.get_info());
    println!(
        "{:?}, canvas {width}x{height}, {name} at {size}, {strokes} strokes of {stroke_ms} ms at {rate} samples a second",
        info.map(|i| i.name)
    );
    // (`HEAVY_SAVE`: saved into a folder of its own and kept saved as the app keeps an artwork,
    // its time-lapse with it)
    if env("HEAVY_SAVE", "0") == "1" {
        let dir = std::env::temp_dir().join(format!("procraft-heavy-realtime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.to_string_lossy().into_owned();
        s.save(path.clone()).unwrap();
        Arc::clone(&s).keep_saved(path, Duration::from_millis(3000), Duration::from_millis(250));
        println!("kept saved in {}", dir.display());
    }
    let samples = (stroke_ms / 1000.0 * rate) as usize;
    // (`HEAVY_POINTER` mouse or finger: at full pressure; `HEAVY_PATH` desk: strokes as the desktop pass posted
    // them, a wave, a line or a zigzag between random points, the window's pixels a sixth smaller)
    let pointer = match env("HEAVY_POINTER", "stylus").as_str() {
        "mouse" => PointerKind::Mouse,
        "finger" => PointerKind::Finger,
        _ => PointerKind::Stylus,
    };
    let mouse = pointer != PointerKind::Stylus;
    let desk = env("HEAVY_PATH", "bands") == "desk";
    let mut seed: u64 = 7;
    let mut random = |from: f32, to: f32| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        from + (to - from) * ((seed >> 33) as f32 / (1u64 << 31) as f32)
    };
    let mut spans = Vec::new();
    for n in 0..strokes {
        let y0 = height as f32 * (0.15 + 0.1 * (n % 8) as f32);
        let k_window = width as f32 / 2160.0;
        let (dx0, dy0, dx1, dy1) =
            (random(150.0, 900.0), random(200.0, 1250.0), random(1200.0, 2080.0), random(200.0, 1250.0));
        let kind = (random(0.0, 3.0) as usize).min(2);
        let begun = Instant::now();
        let t0 = begun.duration_since(started).as_secs_f64() * 1e3;
        let point = |k: usize| {
            let along = k as f32 / samples.max(1) as f32;
            let (x, y) = if desk {
                let (x, mut y) = (dx0 + (dx1 - dx0) * along, dy0 + (dy1 - dy0) * along);
                let reach = (dy1 - dy0).abs().max(0.0) + 200.0;
                match kind {
                    0 => y += (along * std::f32::consts::PI * 4.0).sin() * 0.35 * reach.max(80.0),
                    2 => {
                        let side = if ((along * 8.0) as usize).is_multiple_of(2) { -1.0 } else { 1.0 };
                        y += side * 0.25 * reach.max(80.0) * (along * 8.0).fract();
                    }
                    _ => {}
                }
                (x * k_window, y * k_window)
            } else {
                let x = width as f32 * if n.is_multiple_of(2) { 0.1 + 0.8 * along } else { 0.9 - 0.8 * along };
                (x, y0 + (along * 9.0).sin() * height as f32 * 0.05)
            };
            PenSample {
                x,
                y,
                pressure: if mouse { 1.0 } else { 0.5 + 0.45 * (along * 7.0).sin().abs() },
                tilt: 0.0,
                orientation: 0.0,
                time_ms: t0 + k as f64 * 1000.0 / rate,
                roll: 0.0,
            }
        };
        println!("  stroke {} begins at {t0:.0} ms", n + 1);
        s.stroke_begin(pointer, vec![point(0)]);
        for k in 1..samples {
            let due = Duration::from_secs_f64(k as f64 / rate);
            std::thread::sleep(due.saturating_sub(begun.elapsed()));
            s.stroke_add(vec![point(k)]);
        }
        let lift = Instant::now();
        s.stroke_end(vec![point(samples)]);
        println!(
            "  stroke {} ends at {:.0} ms (stroke_end {:.1} ms)",
            n + 1,
            lift.duration_since(started).as_secs_f64() * 1e3,
            ms(lift)
        );
        std::thread::sleep(Duration::from_millis(400));
        spans.push((t0, started.elapsed().as_secs_f64() * 1e3));
    }
    stop.store(true, Ordering::Relaxed);
    let taken = frames.join().unwrap_or_default();
    for (n, (from, to)) in spans.iter().enumerate() {
        let mut these: Vec<f64> = taken.iter().filter(|(at, ..)| at >= from && at < to).map(|(_, t, _)| *t).collect();
        these.sort_by(f64::total_cmp);
        let slow = these.iter().filter(|&&t| t > 100.0).count();
        let gpu_most = taken.iter().filter(|(at, ..)| at >= from && at < to).map(|(.., g)| *g).fold(0.0, f64::max);
        println!(
            "stroke {:2}: {} frames, med {:5.1} p95 {:6.1} max {:6.1} ms, GPU at most {gpu_most:6.1} ms, over 100 ms {slow}",
            n + 1,
            these.len(),
            these.get(these.len() / 2).copied().unwrap_or(0.0),
            these.get(these.len() * 95 / 100).copied().unwrap_or(0.0),
            these.last().copied().unwrap_or(0.0),
        );
    }
}
