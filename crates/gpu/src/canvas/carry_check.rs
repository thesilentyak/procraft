//! Whether a GPU lays a dual brush's carried part as the plain rule pass lays it (see
//! [`super::Carried`]): checked once on each device, before the first stroke that would carry a
//! part, and the carry left off where they differ.
//!
//! Laid in one lot from nothing, a carried part starts where the plain pass starts and adds the
//! same stamps up the same way: only the full floats it keeps besides are new. Yet on the Adreno
//! 750 (Vulkan; its OpenGL ES failed the carry's tests as well) the carried pass's half floats came
//! out wrong once it kept the paint's own bits beside them, its shader compiler's doing as far as
//! can be told: a dual brush built up by its rule whose mode goes by its second's strength drew
//! next to nothing there, a preview 562 pixels where the plain pass drew 2759, a stroke 292 where it
//! drew 5012. Kept the half floats' bits instead, the pass drew as the plain pass does. A GPU whose
//! check finds the two apart (or that can't be checked) lays such strokes by the plain pass, as
//! before there was a carry: laid whole again as they end.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

use procraft_engine::brush::{GrainMode, Rendering};
use procraft_engine::document::Document;
use procraft_engine::geom::Vec2;
use procraft_engine::stroke::Stamp;

use super::{Canvas, SecondStroke, StrokeStyle, StrokeTool};
use crate::{Gpu, GpuError};

/// How a test makes the carried rule pass go wrong on a device (see [`Gpu::emulate_carry_fault`]).
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarryFault {
    /// As this GPU lays it.
    None,
    /// Its half floats come out a quarter of the paint laid, as a compiler that gets them wrong
    /// left them (the Adreno's drew next to nothing of a dual brush cut by its second).
    Miscompiled,
    /// The check can't be made (as a GPU that fails a readback, or loses its device, can't).
    CheckFails,
}

impl CarryFault {
    fn from_u8(v: u8) -> CarryFault {
        match v {
            1 => CarryFault::Miscompiled,
            2 => CarryFault::CheckFails,
            _ => CarryFault::None,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            CarryFault::None => 0,
            CarryFault::Miscompiled => 1,
            CarryFault::CheckFails => 2,
        }
    }
}

/// A device's check (see the module's note), shared with its clones.
#[derive(Default)]
pub(crate) struct CarryCheck {
    /// Whether parts are carried on the device: the check's finding, once made.
    carries: OnceLock<bool>,
    /// A test's fault (see [`CarryFault`]).
    fault: AtomicU8,
}

impl Gpu {
    /// Whether a dual brush's part laid by its rule is carried from lot to lot on this device (see
    /// the module's note): checked the first time it is asked, once a run (83 to 112 ms on the
    /// Adreno 750, 45 to 115 ms on an RTX 5060).
    pub(crate) fn carries_rule(&self) -> bool {
        *self.carry.carries.get_or_init(|| carry_checked(self))
    }

    /// [`Gpu::carries_rule`], checking now if it hasn't been. For tests.
    #[doc(hidden)]
    pub fn check_carry(&self) -> bool {
        self.carries_rule()
    }

    /// What the check found on this device, if it has been made. For tests.
    #[doc(hidden)]
    pub fn carry_checked(&self) -> Option<bool> {
        self.carry.carries.get().copied()
    }

    /// Takes the check as come to `carries` without making it, unless it has been made: for a
    /// test laying the carried pass as it is (or the plain pass) on any GPU.
    #[doc(hidden)]
    pub fn settle_carry(&self, carries: bool) {
        let _ = self.carry.carries.set(carries);
    }

    /// Makes the carried rule pass go wrong on this device as `fault` says: its canvases'
    /// pipelines made from now on (the first canvas makes them) lay it so. For tests on GPUs
    /// that lay it right.
    #[doc(hidden)]
    pub fn emulate_carry_fault(&self, fault: CarryFault) {
        self.carry.fault.store(fault.as_u8(), Ordering::Relaxed);
    }

    /// The fault a test has made (see [`Gpu::emulate_carry_fault`]).
    pub(crate) fn carry_fault(&self) -> CarryFault {
        CarryFault::from_u8(self.carry.fault.load(Ordering::Relaxed))
    }
}

/// The rule shader's source as a test's fault has it (see [`CarryFault::Miscompiled`]): the carried
/// pass's half floats a quarter of the paint it laid and keeps.
pub(super) fn miscompiled(source: &str) -> String {
    source.replacen("    let halved = halves(paint);\n", "    let halved = halves(paint * 0.25);\n", 1)
}

/// The check's canvas, a side.
const SIDE: u32 = 64;
/// The check's stroke's stamps, each part.
const STAMPS: usize = 48;
/// How far apart a channel of the two may come (as a level of 8 bits) and still be the same: the
/// two passes are compiled apart, and a compiler may fuse a sum otherwise in each.
const SAME_WITHIN: f32 = 1.0 / 255.0;

/// Whether this GPU lays a carried part as the plain pass does, said in the log with what it took.
fn carry_checked(gpu: &Gpu) -> bool {
    let started = Instant::now();
    let checked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| apart(gpu)));
    let ms = started.elapsed().as_secs_f64() * 1e3;
    match checked {
        Ok(Ok((0, most))) => {
            log::info!(
                "the carried rule pass matches on this GPU: carry on (by {most:.5} at most; checked in {ms:.1} ms)"
            );
            true
        }
        Ok(Ok((texels, most))) => {
            log::warn!(
                "the carried rule pass differs on this GPU: carry off ({texels} of {} texels apart, by {most:.3} at \
                 most; checked in {ms:.1} ms)",
                2 * SIDE * SIDE
            );
            false
        }
        Ok(Err(e)) => {
            log::warn!("the carried rule pass couldn't be checked on this GPU ({e}): carry off (in {ms:.1} ms)");
            false
        }
        Err(panic) => {
            log::warn!(
                "the carried rule pass couldn't be checked on this GPU (it panicked: {}): carry off (in {ms:.1} ms)",
                crate::panic_message(panic.as_ref())
            );
            false
        }
    }
}

/// How many texels of the check's stroke's two parts the carried pass lays apart from the plain
/// pass, and by how much at most (see [`SAME_WITHIN`]): one small dual stroke, both parts built up
/// by their rules (one gradually, through a shape and grain; one towards its transfer, round),
/// laid at once from nothing.
fn apart(gpu: &Gpu) -> Result<(usize, f32), GpuError> {
    if gpu.carry_fault() == CarryFault::CheckFails {
        return Err(GpuError::Failed("a test's check failing".to_owned()));
    }
    let (carried, plain) = (laid(gpu, true)?, laid(gpu, false)?);
    if carried.len() != plain.len() {
        return Err(GpuError::Failed("the check's two strokes read back unlike".to_owned()));
    }
    let (mut texels, mut most) = (0, 0.0f32);
    for (a, b) in carried.as_chunks::<8>().0.iter().zip(plain.as_chunks::<8>().0) {
        let channels = a.as_chunks::<2>().0.iter().zip(b.as_chunks::<2>().0);
        let off = channels.fold(0.0f32, |m, (x, y)| {
            let d = (half(*x) - half(*y)).abs();
            // (a NaN on either side is as far apart as can be)
            if d.is_nan() { f32::INFINITY } else { m.max(d) }
        });
        if off > SAME_WITHIN {
            texels += 1;
        }
        most = most.max(off);
    }
    Ok((texels, most))
}

/// The check's stroke laid in one frame on a canvas of its own, carried or by the plain pass: its
/// two parts' half floats, the stroke's and then its second's.
fn laid(gpu: &Gpu, carried: bool) -> Result<Vec<u8>, GpuError> {
    let mut doc = Document::new(SIDE, SIDE);
    // (without the brushes' images: a shape and grain read the stand-ins, all full)
    let mut canvas = Canvas::make(gpu, &doc, None, false)?;
    canvas.fill_rest(gpu, &doc);
    canvas.checking_carry = true;
    canvas.carry_refused = !carried;
    let first = StrokeStyle {
        rendering: Rendering::Gradual,
        shape: Some(0),
        grain: Some(0),
        grain_mode: GrainMode::Moving,
        grain_depth: 0.8,
        grain_scale: 20.0,
        ..StrokeStyle::new(StrokeTool::Paint, [0.7, 0.25, 0.1], 1.0)
    };
    let second = StrokeStyle {
        rendering: Rendering::Capped,
        hardness: 0.6,
        ..StrokeStyle::new(StrokeTool::Paint, [0.1, 0.3, 0.8], 1.0)
    };
    canvas.begin_dual_stroke(
        doc.current(),
        first,
        SecondStroke { style: second, mode: 1, threshold: -1.0, classic: false },
    );
    if canvas.carries(false) != carried || canvas.carries(true) != carried {
        return Err(GpuError::Failed("the check's stroke isn't laid as asked".to_owned()));
    }
    canvas.add_second_stamps(&stamps(true));
    canvas.add_stamps(&stamps(false));
    canvas.render(gpu, &mut doc, None);
    let mut out = read_halves(gpu, &canvas.stroke)?;
    let Some(second) = canvas.second_stroke.made() else {
        return Err(GpuError::Failed("the check's stroke drew no second".to_owned()));
    };
    out.extend(read_halves(gpu, second)?);
    Ok(out)
}

/// The check's stroke's stamps, of all sizes, turns, squashes, strengths and transfers along a
/// wave across the canvas (its second's a half step on and a little apart).
fn stamps(second: bool) -> Vec<Stamp> {
    (0..STAMPS)
        .map(|i| {
            let t = i as f32 + if second { 0.5 } else { 0.0 };
            let x = 8.0 + t;
            let y = 32.0 + (t * 0.21).sin() * 14.0 + if second { (t * 0.31).cos() * 4.0 } else { 0.0 };
            let radius = 3.0 + 7.0 * (t * 0.13).sin().abs() + if second { 2.0 } else { 0.0 };
            Stamp {
                angle: t * 0.37,
                roundness: 0.4 + 0.6 * (t * 0.11).sin().abs(),
                transfer: 0.3 + 0.7 * (t * 0.07).cos().abs(),
                grain_offset: Vec2::new(t * 1.7, t * 0.9),
                time_ms: f64::from(t) * 4.0,
                ..Stamp::round(Vec2::new(x, y), radius, 0.15 + 0.8 * (t * 0.061).cos().abs())
            }
        })
        .collect()
}

/// A texture of half floats, four a texel, read back whole: its bytes, rows without padding.
fn read_halves(gpu: &Gpu, texture: &wgpu::Texture) -> Result<Vec<u8>, GpuError> {
    let (width, height) = (texture.width(), texture.height());
    let row = width * 8;
    let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let size = u64::from(padded) * u64::from(height);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("carry check"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let invalid = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("carry check") });
    gpu.copy_texture_to_buffer(
        &mut encoder,
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    gpu.queue.submit([encoder.finish()]);
    if let Some(e) = pollster::block_on(invalid.pop()) {
        return Err(GpuError::Readback(e.to_string()));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    crate::poll(&gpu.device, wgpu::PollType::wait_indefinitely())?;
    match rx.try_recv() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(GpuError::Readback(e.to_string())),
        Err(e) => return Err(GpuError::Readback(e.to_string())),
    }
    let out = {
        let mapped = buffer.slice(..).get_mapped_range().map_err(|e| GpuError::Readback(e.to_string()))?;
        mapped.chunks(padded as usize).flat_map(|r| r.get(..row as usize).unwrap_or_default().to_vec()).collect()
    };
    buffer.unmap();
    Ok(out)
}

/// The half float in `bytes` (little-endian), as a float.
fn half(bytes: [u8; 2]) -> f32 {
    let bits = u16::from_le_bytes(bytes);
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff);
    sign * match exponent {
        0 => fraction * (-24f32).exp2(),
        31 if fraction == 0.0 => f32::INFINITY,
        31 => f32::NAN,
        _ => (1.0 + fraction / 1024.0) * ((exponent - 15) as f32).exp2(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_floats_read_as_their_values() {
        let h = |bits: u16| half(bits.to_le_bytes());
        assert_eq!(h(0x3c00), 1.0);
        assert_eq!(h(0xc000), -2.0);
        assert_eq!(h(0x3800), 0.5);
        assert_eq!(h(0x0001), (-24f32).exp2());
        assert_eq!(h(0x7c00), f32::INFINITY);
        assert!(h(0x7e00).is_nan());
        assert_eq!(h(0x0000), 0.0);
    }
}
