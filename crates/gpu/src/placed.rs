//! Copies between textures and buffers, made as Direct3D 12 without unrestricted copy pitches
//! makes them where a test asks (see [`Gpu::emulate_placed_copies`]), or as a driver that writes
//! a copy's rows whole (see [`Gpu::emulate_whole_row_copies`]).
//!
//! Direct3D 12 without them (on Windows 10, and with older drivers, as many integrated GPUs have)
//! places a texture's rows in a buffer only at offsets that are multiples of [`PLACEMENT`]. wgpu
//! copies from a texture to any other offset through a buffer of its own, holding whatever its
//! memory held, and copies that back whole: the rest of each row but the last with it, so what
//! else the buffer held between the copy's rows is written over. A copy from a buffer to such an
//! offset goes through a buffer of its own too, which holds just what it copies.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::Gpu;

/// The multiple of bytes a copy between a texture and a buffer is placed at in the buffer on
/// Direct3D 12 without unrestricted copy pitches (see the module's note).
pub(crate) const PLACEMENT: u64 = 512;

/// Whether a device's copies are made as placed (see [`Gpu::emulate_placed_copies`]), and how many
/// from a texture went through a buffer of their own.
#[derive(Default)]
pub(crate) struct Placed {
    on: AtomicBool,
    /// Copies from a texture whose rows leave room between them in the buffer are made as a
    /// driver that writes whole rows makes them (see [`Gpu::emulate_whole_row_copies`]).
    rows: AtomicBool,
    staged: AtomicU64,
}

/// Bytes a copy of `size` from or into a buffer laid out as `layout` spans in it, from its first
/// pixel to its last (pixels `block` bytes): what the buffer of its own takes.
fn spans(layout: wgpu::TexelCopyBufferLayout, size: wgpu::Extent3d, block: u32) -> u64 {
    let last = u64::from(size.width) * u64::from(block);
    let row = layout.bytes_per_row.map_or(last, u64::from);
    let rows = layout.rows_per_image.unwrap_or(size.height);
    let image = row * u64::from(size.height.saturating_sub(1)) + last;
    row * u64::from(rows) * u64::from(size.depth_or_array_layers.saturating_sub(1)) + image
}

impl Gpu {
    /// Copies between textures and buffers made from now on as Direct3D 12 without unrestricted
    /// copy pitches makes them (`on`), on this device and its clones (see the module's note): a
    /// copy from a texture to an offset it doesn't place goes through a buffer of its own holding
    /// every byte 255, and all of that is copied back. For tests on other GPUs.
    #[doc(hidden)]
    pub fn emulate_placed_copies(&self, on: bool) {
        self.placed.on.store(on, Ordering::Relaxed);
    }

    /// Copies from a texture into a buffer made from now on as a driver that writes a copy's rows
    /// whole makes them (`on`), on this device and its clones: one whose rows leave room between
    /// them in the buffer (a part of a wider picture's rows) goes through a buffer of its own
    /// holding every byte 255, all of which is copied back, so the rest of each of its rows but
    /// the last is written over, wherever it starts. For tests on other GPUs.
    #[doc(hidden)]
    pub fn emulate_whole_row_copies(&self, on: bool) {
        self.placed.rows.store(on, Ordering::Relaxed);
    }

    /// How many copies from a texture went through a buffer of their own, which writes over the
    /// rest of their rows but the last (see [`Gpu::emulate_placed_copies`]).
    #[doc(hidden)]
    pub fn placed_copies_staged(&self) -> u64 {
        self.placed.staged.load(Ordering::Relaxed)
    }

    /// Whether a copy with `layout` goes through a buffer of its own (see
    /// [`Gpu::emulate_placed_copies`]): one from or into a buffer at an offset not placed, a
    /// whole number of 4-byte words long (as a copy between buffers is).
    fn staged(&self, layout: wgpu::TexelCopyBufferLayout, bytes: u64) -> bool {
        self.placed.on.load(Ordering::Relaxed)
            && !layout.offset.is_multiple_of(PLACEMENT)
            && bytes.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
    }

    /// `encoder.copy_texture_to_buffer`, made as Direct3D 12 without unrestricted copy pitches
    /// makes it, or as a driver that writes a copy's rows whole, where a test asks (see
    /// [`Gpu::emulate_placed_copies`], [`Gpu::emulate_whole_row_copies`]).
    pub(crate) fn copy_texture_to_buffer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        from: wgpu::TexelCopyTextureInfo<'_>,
        into: wgpu::TexelCopyBufferInfo<'_>,
        size: wgpu::Extent3d,
    ) {
        let layout = into.layout;
        let block = from.texture.format().block_copy_size(Some(from.aspect)).unwrap_or(4);
        let bytes = spans(layout, size, block);
        // (rows with room between them, as a driver writing them whole writes over)
        let gaps = size.height > 1 && layout.bytes_per_row.is_some_and(|row| row > size.width * block);
        let whole_rows = self.placed.rows.load(Ordering::Relaxed) && gaps;
        if !whole_rows && !self.staged(layout, bytes) {
            encoder.copy_texture_to_buffer(from, into, size);
            return;
        }
        self.placed.staged.fetch_add(1, Ordering::Relaxed);
        let filler = vec![255; usize::try_from(bytes).unwrap_or(0)];
        let usage = wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
        let staged = crate::canvas::buffer_init(&self.device, "placed copy", &filler, usage);
        let at_start = wgpu::TexelCopyBufferLayout { offset: 0, ..layout };
        encoder.copy_texture_to_buffer(from, wgpu::TexelCopyBufferInfo { buffer: &staged, layout: at_start }, size);
        encoder.copy_buffer_to_buffer(&staged, 0, into.buffer, layout.offset, bytes);
    }

    /// `encoder.copy_buffer_to_texture`, made as Direct3D 12 without unrestricted copy pitches
    /// makes it where a test asks (see [`Gpu::emulate_placed_copies`]), and as the driver of a GPU
    /// whose new textures start stale (see [`Gpu::emulate_stale_textures`]).
    pub(crate) fn copy_buffer_to_texture(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        from: wgpu::TexelCopyBufferInfo<'_>,
        into: wgpu::TexelCopyTextureInfo<'_>,
        size: wgpu::Extent3d,
    ) {
        if let Some((stale, copied)) = self.stale_after_buffer_copy(&into, size) {
            self.copy_buffer_to_texture_placed(encoder, from, copied.as_image_copy(), size);
            self.copy_as_the_driver(encoder, &copied, into, size, &stale);
            return;
        }
        self.copy_buffer_to_texture_placed(encoder, from, into, size);
    }

    /// [`Gpu::copy_buffer_to_texture`] as Direct3D 12 without unrestricted copy pitches makes it,
    /// whatever the texture held before.
    fn copy_buffer_to_texture_placed(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        from: wgpu::TexelCopyBufferInfo<'_>,
        into: wgpu::TexelCopyTextureInfo<'_>,
        size: wgpu::Extent3d,
    ) {
        let layout = from.layout;
        let block = into.texture.format().block_copy_size(Some(into.aspect)).unwrap_or(4);
        let bytes = spans(layout, size, block);
        if !self.staged(layout, bytes) {
            encoder.copy_buffer_to_texture(from, into, size);
            return;
        }
        let staged = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("placed copy"),
            size: bytes,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_buffer_to_buffer(from.buffer, layout.offset, &staged, 0, bytes);
        let at_start = wgpu::TexelCopyBufferLayout { offset: 0, ..layout };
        encoder.copy_buffer_to_texture(wgpu::TexelCopyBufferInfo { buffer: &staged, layout: at_start }, into, size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a copy spans in a buffer: rows apart as laid out, the last row only as wide as the
    /// copy, as wgpu's buffer of its own for it is.
    #[test]
    fn a_copy_spans_its_rows_but_the_last_one_s_padding() {
        let layout =
            |bytes_per_row, rows_per_image| wgpu::TexelCopyBufferLayout { offset: 300, bytes_per_row, rows_per_image };
        let size = |width, height, depth_or_array_layers| wgpu::Extent3d { width, height, depth_or_array_layers };
        assert_eq!(spans(layout(Some(1024), None), size(10, 3, 1), 4), 2 * 1024 + 40);
        assert_eq!(spans(layout(Some(256), Some(1)), size(1, 1, 1), 4), 4);
        assert_eq!(spans(layout(None, None), size(7, 1, 1), 8), 56);
        assert_eq!(spans(layout(Some(512), Some(4)), size(16, 2, 3), 4), 2 * 4 * 512 + 512 + 64);
    }
}
