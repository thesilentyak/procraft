//! Filter layers (interface 17): a layer with no paint of its own that shows what is below it run
//! through a plugin's filter. The filter is a filter's one function (`pixel`), joined to a template
//! of its own (`shaders/plugin_filter_layer_17.wgsl`) that reads the picture below the layer as it
//! stands where the layer comes in the stack: everything below it in its group, or, clipped in a
//! clipping group, its base and what is clipped to that below it. What it makes goes onto that
//! picture as far as the layer's opacity, mask and clipping say, by its blend mode (`fs_filter`).
//!
//! It is made again whenever that part of the canvas is drawn, so its loops have an eighth of the
//! budget a filter on a layer has, and a pass of it is cut into draws no bigger than a filter's
//! pieces are. A filter says how far from a pixel it reads (`layer_at` is held to that), and the
//! canvas draws that much more round what changed, as many times over as filter layers show; one
//! that reads anywhere has the whole canvas drawn again for any change.
//!
//! A filter with an editor (interface 18: curves, or a gradient) reads what the editor set on the
//! layer as `table(row, x)`: the canvas bakes each such layer's lists into a small texture of rows
//! of 256 before it composites, again only as they change.

use std::collections::HashMap;
use std::sync::Arc;

use procraft_engine::document::{Document, Layer, LayerFilter, LayerId};
use procraft_engine::geom::IRect;
use procraft_engine::tone;

use super::plugin_filter::{PROBE_SIZE, rows_at_once};
use super::{
    CLIP_ATOP, CLIP_BY_BASE, CLIP_NONE, Canvas, FilterUniform, LayerUniform, Look, Stack, UniformData, copy_at, extent,
    read_texture, render_pass, scissor,
};
use crate::{FilterProgram, FilterRun, Gpu};

/// How many values a filter layer's filter reads, at most (its uniform's places).
pub const MOST_LAYER_FILTER_VALUES: usize = 8;

/// A filter layer's filter, ready on this device, as the app gives it for each `<plugin id>.<key>`.
#[derive(Clone)]
pub struct LayerFilterProgram {
    pub program: Arc<FilterProgram>,
    /// Its values' keys, in the order its code reads them, and what each is where a layer keeps
    /// none of its own.
    pub values: Vec<(String, f32)>,
    pub reach: FilterReach,
    /// What its editor sets, if it has one (interface 18), and the key the layer keeps it by.
    pub table: Option<(String, FilterTable)>,
}

/// What a filter layer's editor sets, baked into rows of 256 for its code (interface 18).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterTable {
    /// Curves: list `<key>.<n>` (x, y points) a row each, its heights in red.
    Curves { channels: u32 },
    /// A gradient: list `<key>` (place, red, green, blue stops), one row of its colours.
    Gradient,
}

/// The most rows a table has.
const MOST_TABLE_ROWS: u32 = 8;

impl FilterTable {
    /// Its rows of 256 RGBA bytes, made of what `filter` keeps under `key`.
    pub fn rows(self, filter: &LayerFilter, key: &str) -> (u32, Vec<u8>) {
        match self {
            FilterTable::Curves { channels } => {
                let channels = channels.clamp(1, MOST_TABLE_ROWS);
                let mut out = Vec::with_capacity(channels as usize * 1024);
                for n in 0..channels {
                    let numbers = filter.table(&format!("{key}.{n}")).unwrap_or_default();
                    let points: Vec<(f32, f32)> = numbers.as_chunks::<2>().0.iter().map(|&[x, y]| (x, y)).collect();
                    out.extend(tone::curve_table(&points).iter().flat_map(|&h| [h, h, h, 255]));
                }
                (channels, out)
            }
            FilterTable::Gradient => (1, gradient_row(filter.table(key).unwrap_or_default())),
        }
    }
}

/// A gradient's stops (place, red, green, blue, 0..1) as 256 colours: the first's below it, the
/// last's above, straight between the two about each place. None: black to white.
fn gradient_row(numbers: &[f32]) -> Vec<u8> {
    let mut stops: Vec<[f32; 4]> =
        numbers.as_chunks::<4>().0.iter().copied().filter(|s| s.iter().all(|n| n.is_finite())).collect();
    stops.sort_by(|a, b| a[0].total_cmp(&b[0]));
    if stops.is_empty() {
        stops = vec![[0.0, 0.0, 0.0, 0.0], [1.0, 1.0, 1.0, 1.0]];
    }
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    (0..256)
        .flat_map(|i| {
            let t = i as f32 / 255.0;
            let after = stops.iter().position(|s| s[0] >= t).unwrap_or(stops.len());
            let c = match (after.checked_sub(1).and_then(|b| stops.get(b)), stops.get(after)) {
                (Some(&[pa, ra, ga, ba]), Some(&[pb, rb, gb, bb])) if pb > pa => {
                    let k = (t - pa) / (pb - pa);
                    [ra + (rb - ra) * k, ga + (gb - ga) * k, ba + (bb - ba) * k]
                }
                (Some(a), _) => [a[1], a[2], a[3]],
                (None, Some(b)) => [b[1], b[2], b[3]],
                (None, None) => [t, t, t],
            };
            [byte(c[0]), byte(c[1]), byte(c[2]), 255]
        })
        .collect()
}

/// A filter layer's table on the GPU, and the lists it was baked from.
pub(super) struct TableTexture {
    made_of: Vec<u8>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// How far from the pixel it makes a filter reads what is below.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FilterReach {
    /// So many canvas pixels each way.
    Within(f32),
    /// A value's (its place among the filter's values) times `times`, plus `plus`, canvas pixels.
    ByValue { value: usize, times: f32, plus: f32 },
    /// Anywhere on the canvas.
    Whole,
}

/// The farthest a filter may say it reads, canvas pixels: past it, it reads anywhere.
const MOST_REACH: f32 = 4096.0;

impl LayerFilterProgram {
    /// The values for a layer keeping `filter`, in the order the code reads them.
    fn values_of(&self, filter: &LayerFilter) -> [f32; MOST_LAYER_FILTER_VALUES] {
        let mut out = [0.0; MOST_LAYER_FILTER_VALUES];
        for (kept, (key, starts)) in out.iter_mut().zip(&self.values) {
            *kept = filter.value(key).unwrap_or(*starts);
        }
        out
    }

    /// How far it reads for a layer keeping `filter`, whole canvas pixels; none: anywhere.
    fn reach_of(&self, filter: &LayerFilter) -> Option<u32> {
        let pixels = match self.reach {
            FilterReach::Within(r) => r,
            FilterReach::ByValue { value, times, plus } => {
                let v = self.values_of(filter).get(value).copied().unwrap_or(0.0);
                v * times + plus
            }
            FilterReach::Whole => return None,
        };
        // not a number, or past what any canvas holds: anywhere
        (pixels.is_finite() && pixels <= MOST_REACH).then(|| pixels.max(0.0).ceil() as u32)
    }
}

/// `rect` grown by `by` each way (an empty one stays empty).
fn grown(rect: IRect, by: u32) -> IRect {
    if rect.is_empty() {
        return rect;
    }
    let by = i32::try_from(by).unwrap_or(i32::MAX / 4).min(i32::MAX / 4);
    IRect::new(
        rect.x0.saturating_sub(by),
        rect.y0.saturating_sub(by),
        rect.x1.saturating_add(by),
        rect.y1.saturating_add(by),
    )
}

/// The parts of `outer` not in `inner` (which is inside it): up to four bands.
fn around(outer: IRect, inner: IRect) -> Vec<IRect> {
    let bands = [
        IRect::new(outer.x0, outer.y0, outer.x1, inner.y0),
        IRect::new(outer.x0, inner.y1, outer.x1, outer.y1),
        IRect::new(outer.x0, inner.y0, inner.x0, inner.y1),
        IRect::new(inner.x1, inner.y0, outer.x1, inner.y1),
    ];
    bands.into_iter().filter(|b| !b.is_empty()).collect()
}

impl Canvas {
    /// Bakes each filter layer's table (interface 18) whose lists changed since it was last baked,
    /// and lets go of those of layers gone or no longer filtered so.
    pub(super) fn prepare_filter_tables(&mut self, gpu: &Gpu, doc: &Document) {
        let mut seen = Vec::new();
        for layer in doc.layers() {
            let Some(filter) = layer.filter.as_ref() else { continue };
            let Some((key, table)) = self.layer_filters.get(&filter.filter).and_then(|p| p.table.clone()) else {
                continue;
            };
            seen.push(layer.id);
            let (rows, bytes) = table.rows(filter, &key);
            if self.filter_tables.get(&layer.id).is_some_and(|t| t.made_of == bytes) {
                continue;
            }
            let texture = gpu.texture(&wgpu::TextureDescriptor {
                label: Some("filter layer table"),
                size: extent(256, rows),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            gpu.write_texture(
                copy_at(&texture, 0, 0),
                &bytes,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(1024), rows_per_image: Some(rows) },
                extent(256, rows),
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.filter_tables.insert(layer.id, TableTexture { made_of: bytes, texture, view });
            // what it changes is drawn again
            self.dirty = self.full();
        }
        self.filter_tables.retain(|id, _| seen.contains(id));
    }

    /// The textures of filter layers' tables, to count.
    pub(super) fn table_textures(&self) -> impl Iterator<Item = &wgpu::Texture> {
        self.filter_tables.values().map(|t| &t.texture)
    }

    /// The filters filter layers run, each by `<plugin id>.<key>` as layers name them: a layer
    /// whose filter isn't here shows nothing. Everything is composited again.
    pub fn set_layer_filters(&mut self, filters: HashMap<String, LayerFilterProgram>) {
        // none for none: nothing composites otherwise, and the caches stay
        if filters.is_empty() && self.layer_filters.is_empty() {
            return;
        }
        self.layer_filters = filters;
        self.invalidate();
    }

    /// Composites the whole canvas again at the next render, caches kept (a filter layer's values
    /// changed: the layer is among what is composited live while it is worked on, and a cache
    /// holding it is made again as its settings differ).
    pub fn composite_again(&mut self) {
        self.dirty = self.full();
    }

    /// Whether filter layers running `name` show what it makes.
    pub fn has_layer_filter(&self, name: &str) -> bool {
        self.layer_filters.contains_key(name)
    }

    /// Tries `program` on a few hundred pixels spread over the composite, as a filter is tried
    /// before it meets a layer: [`FilterRun::Spent`] if one of them ran out of its budget, which a
    /// filter that never ends does; else [`FilterRun::Drew`].
    pub fn try_layer_filter(&self, gpu: &Gpu, program: &FilterProgram) -> FilterRun {
        let device = &gpu.device;
        if gpu.is_lost() {
            return FilterRun::Lost;
        }
        let started = std::time::Instant::now();
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("filter probe") });
        let mut uniforms = UniformData::default();
        let (w, h) = (self.width as f32, self.height as f32);
        let probe = PROBE_SIZE as f32;
        let uniform = FilterUniform { a: [0.0; 4], b: [0.0; 4], size: [w, h, 0.0, 0.0], c: [probe, probe, -1.0, 0.0] };
        let offset = uniforms.push(&uniform);
        let bind = self.filter_source(device, &self.composite_view);
        let before = self.filter_before(device, None);
        let target = program.probe_target().create_view(&wgpu::TextureViewDescriptor::default());
        {
            let mut pass =
                render_pass(&mut encoder, "filter layer probe", &target, wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT));
            pass.set_pipeline(program.probe_pipeline());
            pass.set_bind_group(0, &bind, &[]);
            pass.set_bind_group(1, &self.uniform_bind, &[offset]);
            pass.set_bind_group(2, &before, &[]);
            pass.draw(0..3, 0..1);
        }
        self.write_uniforms(gpu, &uniforms);
        gpu.queue.submit([encoder.finish()]);
        let looked = match read_texture(gpu, program.probe_target(), PROBE_SIZE, PROBE_SIZE) {
            Ok(pixels) => pixels,
            Err(_) => return if gpu.is_lost() { FilterRun::Lost } else { FilterRun::Cut },
        };
        if looked.as_chunks::<4>().0.iter().any(|[red, ..]| *red > 127) {
            return FilterRun::Spent;
        }
        FilterRun::Drew(started.elapsed())
    }

    /// How far a change reaches through the filter layers in `doc` that show: canvas pixels, the
    /// reach of each added up; none: anywhere (one reads anywhere).
    pub(super) fn filter_reach(&self, doc: &Document) -> Option<u32> {
        let mut total = 0_u32;
        for layer in doc.layers() {
            let Some(filter) = layer.filter.as_ref() else { continue };
            if !doc.in_picture(layer.id) || layer.opacity <= 0.0 {
                continue;
            }
            let Some(program) = self.layer_filters.get(&filter.filter) else { continue };
            total = total.saturating_add(program.reach_of(filter)?);
        }
        Some(total)
    }

    /// What a change within `rect` changes of the composite: `rect`, grown by how far filter layers
    /// reach; the whole canvas if one reads anywhere.
    pub(super) fn reached(&self, doc: &Document, rect: IRect) -> IRect {
        if rect.is_empty() {
            return rect;
        }
        match self.filter_reach(doc) {
            Some(0) => rect,
            Some(r) => grown(rect, r).intersect(self.full()),
            None => self.full(),
        }
    }

    /// Where the composite is made again for `rect` (see [`Canvas::reached`]) to come out right:
    /// that much further round again, filter layers reading what is below about each pixel.
    pub(super) fn filter_work(&self, doc: &Document, rect: IRect) -> IRect {
        match self.filter_reach(doc) {
            Some(0) => rect,
            Some(r) => grown(rect, r).intersect(self.full()),
            None => self.full(),
        }
    }

    /// Keeps what the composite holds in `work` about `rect`, before what is round `rect` is made
    /// again for what is in it, wrong at the edges.
    pub(super) fn keep_around(&self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, work: IRect, rect: IRect) {
        let (keep, _) = self.filter_keep.get(gpu);
        for band in around(work, rect) {
            gpu.copy_texture_to_texture(
                encoder,
                copy_at(&self.composite, band.x0, band.y0),
                copy_at(keep, band.x0, band.y0),
                extent(band.width(), band.height()),
            );
        }
    }

    /// Puts back what [`Canvas::keep_around`] kept.
    pub(super) fn put_back_around(&self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, work: IRect, rect: IRect) {
        let (keep, _) = self.filter_keep.get(gpu);
        for band in around(work, rect) {
            gpu.copy_texture_to_texture(
                encoder,
                copy_at(keep, band.x0, band.y0),
                copy_at(&self.composite, band.x0, band.y0),
                extent(band.width(), band.height()),
            );
        }
    }

    /// What a filter's draw reads: `source` as what is below.
    fn filter_source(&self, device: &wgpu::Device, source: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("filter layer source"),
            layout: &self.pipelines.transform_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(source) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&self.everything) },
            ],
        })
    }

    /// The adjustments' last group: a filter layer's table (interface 18) where `layer` has one,
    /// else any texture, unread.
    fn filter_before(&self, device: &wgpu::Device, layer: Option<LayerId>) -> wgpu::BindGroup {
        let table = layer.and_then(|id| self.filter_tables.get(&id)).map_or(&self.everything, |t| &t.view);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("filter layer table"),
            layout: &self.pipelines.before_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(table) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.everything) },
            ],
        })
    }

    /// Filter layer `layer` onto a stack whose result is in its first texture (`current` 0) or its
    /// second (1): what the stack holds run through the layer's filter, then blended back onto it.
    /// `role` is CLIP_NONE, or CLIP_ATOP clipped in a clipping group. Nothing while its filter
    /// isn't given, or it doesn't show.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn composite_filter_layer(
        &self,
        gpu: &Gpu,
        doc: &Document,
        encoder: &mut wgpu::CommandEncoder,
        uniforms: &mut UniformData,
        rect: IRect,
        layer: &Layer,
        look: Look,
        role: u32,
        stack: &Stack<'_>,
        current: &mut usize,
    ) {
        let device = &gpu.device;
        let Some(filter) = layer.filter.as_ref() else { return };
        let Some(program) = self.layer_filters.get(&filter.filter) else { return };
        let clip = doc.clip_base(layer.id).filter(|_| role == CLIP_NONE);
        let clip_visible = clip.is_none_or(|id| doc.layer(id).is_some_and(|l| l.visible));
        let opacity = layer.opacity.clamp(0.0, 1.0) * look.opacity;
        if !layer.visible || !clip_visible || opacity <= 0.0 || rect.is_empty() {
            return;
        }
        // what the stack holds through the filter, in a draw no bigger than a filter's piece
        let below = if *current == 0 { stack.first.1 } else { stack.second().1 };
        let (_, out) = self.filter_out.get(gpu);
        let values = program.values_of(filter);
        let [a @ .., _, _, _, _] = values;
        let [_, _, _, _, b @ ..] = values;
        let (w, h) = (self.width as f32, self.height as f32);
        let reach = program.reach_of(filter).map_or(-1.0, |r| r as f32);
        let probe = PROBE_SIZE as f32;
        let uniform = FilterUniform { a, b, size: [w, h, 0.0, 0.0], c: [probe, probe, reach, 0.0] };
        let offset = uniforms.push(&uniform);
        let source = self.filter_source(device, below);
        let before = self.filter_before(device, Some(layer.id));
        let rows = rows_at_once(rect.width(), program.program.worst()).max(1);
        {
            let mut pass = render_pass(encoder, "filter layer", out, wgpu::LoadOp::Load);
            pass.set_pipeline(program.program.draw_pipeline());
            pass.set_bind_group(0, &source, &[]);
            pass.set_bind_group(1, &self.uniform_bind, &[offset]);
            pass.set_bind_group(2, &before, &[]);
            let mut y = rect.y0;
            while y < rect.y1 {
                let count = i32::try_from(rows).unwrap_or(i32::MAX).min(rect.y1 - y);
                scissor(&mut pass, IRect::new(rect.x0, y, rect.x1, y + count));
                pass.draw(0..3, 0..1);
                y += count;
            }
        }
        // then back onto what it was made from, into the stack's other texture
        let clip_view = clip.and_then(|id| self.layers.get(&id)).map_or(&self.stroke_view, |c| &c.view);
        let clip_mask_view = clip
            .and_then(|b| doc.mask_of(b))
            .filter(|m| doc.layer(*m).is_some_and(|l| l.visible))
            .and_then(|m| self.layers.get(&m))
            .map(|m| &m.view);
        let mask = doc.mask_of(layer.id).filter(|m| doc.layer(*m).is_some_and(|l| l.visible));
        let mask_view = mask.and_then(|m| self.layers.get(&m)).map(|m| &m.view);
        let uniform = LayerUniform {
            opacity,
            blend: layer.blend.id(),
            clipped: if clip.is_some() { CLIP_BY_BASE } else { role },
            clip_masked: if clip_mask_view.is_some() { 1.0 } else { 0.0 },
            masked: if mask_view.is_some() { 1.0 } else { 0.0 },
            ..Default::default()
        };
        let offset = uniforms.push(&uniform);
        let second = stack.second().1;
        let first = stack.first.1;
        let (backdrop, target) = if *current == 0 { (first, second) } else { (second, first) };
        let bind = self.quad_masked(
            gpu,
            [out, &self.stroke_view, clip_view, backdrop],
            mask_view.unwrap_or(&self.stroke_view),
            clip_mask_view.unwrap_or(&self.stroke_view),
        );
        let mut pass = render_pass(encoder, "filter layer blend", target, wgpu::LoadOp::Load);
        scissor(&mut pass, rect);
        pass.set_pipeline(&self.pipelines.filter_layer);
        pass.set_bind_group(0, &bind, &[]);
        pass.set_bind_group(1, &self.uniform_bind, &[offset]);
        pass.draw(0..3, 0..1);
        drop(pass);
        *current = 1 - *current;
    }
}

impl Canvas {
    /// Filter layer `upper` of `doc` merged down into layer `lower`: its filter run over `lower`
    /// alone, then into it as far as `upper`'s mask and opacity say, by its blend mode (`clipped`
    /// CLIP_ATOP where it is clipped to `lower`). Nothing changes while its filter isn't given.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn filter_into(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        uniforms: &mut UniformData,
        doc: &Document,
        upper: &Layer,
        lower: LayerId,
        clipped: u32,
    ) -> Option<()> {
        let device = &gpu.device;
        let filter = upper.filter.as_ref()?;
        let program = self.layer_filters.get(&filter.filter)?;
        let low = self.layers.get(&lower)?;
        let full = self.full();
        let (_, out) = self.filter_out.get(gpu);
        let values = program.values_of(filter);
        let [a @ .., _, _, _, _] = values;
        let [_, _, _, _, b @ ..] = values;
        let (w, h) = (self.width as f32, self.height as f32);
        let reach = program.reach_of(filter).map_or(-1.0, |r| r as f32);
        let probe = PROBE_SIZE as f32;
        let offset = uniforms.push(&FilterUniform { a, b, size: [w, h, 0.0, 0.0], c: [probe, probe, reach, 0.0] });
        let source = self.filter_source(device, &low.view);
        let before = self.filter_before(device, Some(upper.id));
        let rows = rows_at_once(self.width, program.program.worst()).max(1);
        {
            let mut pass = render_pass(encoder, "filter merged", out, wgpu::LoadOp::Load);
            pass.set_pipeline(program.program.draw_pipeline());
            pass.set_bind_group(0, &source, &[]);
            pass.set_bind_group(1, &self.uniform_bind, &[offset]);
            pass.set_bind_group(2, &before, &[]);
            let mut y = 0;
            while y < full.y1 {
                let count = i32::try_from(rows).unwrap_or(i32::MAX).min(full.y1 - y);
                scissor(&mut pass, IRect::new(0, y, full.x1, y + count));
                pass.draw(0..3, 0..1);
                y += count;
            }
        }
        let mask = doc.mask_of(upper.id).filter(|m| doc.layer(*m).is_some_and(|l| l.visible));
        let mask_view = mask.and_then(|m| self.layers.get(&m)).map(|m| &m.view);
        let uniform = LayerUniform {
            opacity: if upper.visible { upper.opacity.clamp(0.0, 1.0) } else { 0.0 },
            blend: upper.blend.id(),
            clipped: if clipped == CLIP_ATOP { CLIP_ATOP } else { CLIP_NONE },
            masked: if mask_view.is_some() { 1.0 } else { 0.0 },
            ..Default::default()
        };
        let offset = uniforms.push(&uniform);
        let bind = self.quad_masked(
            gpu,
            [out, &self.stroke_view, &self.stroke_view, &low.view],
            mask_view.unwrap_or(&self.stroke_view),
            &self.stroke_view,
        );
        let (scratch, scratch_view) = self.scratch.get(gpu);
        {
            let mut pass = render_pass(encoder, "filter merged blend", scratch_view, wgpu::LoadOp::Load);
            pass.set_pipeline(&self.pipelines.filter_layer);
            pass.set_bind_group(0, &bind, &[]);
            pass.set_bind_group(1, &self.uniform_bind, &[offset]);
            pass.draw(0..3, 0..1);
        }
        gpu.copy_texture_to_texture(
            encoder,
            copy_at(scratch, 0, 0),
            copy_at(&low.texture, 0, 0),
            extent(self.width, self.height),
        );
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gradient_s_stops_make_a_row_of_its_colours_and_curves_a_row_each() {
        // black, red at the middle, white: red half way, pink three quarters of the way
        let row = gradient_row(&[1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0]);
        let at = |i: usize| [row[i * 4], row[i * 4 + 1], row[i * 4 + 2], row[i * 4 + 3]];
        assert_eq!((at(0), at(255)), ([0, 0, 0, 255], [255, 255, 255, 255]));
        assert_eq!(at(128)[..2], [255, 1]);
        assert!(at(191)[0] == 255 && at(191)[1].abs_diff(128) <= 1, "{:?}", at(191));
        assert_eq!(gradient_row(&[])[..4], [0, 0, 0, 255], "none: black to white");
        // curves: a row each, the identity where none is kept
        let mut filter = LayerFilter::new("t.curves", []);
        assert!(filter.set_table("curves.1", &[0.0, 1.0, 1.0, 0.0]));
        let (rows, bytes) = FilterTable::Curves { channels: 3 }.rows(&filter, "curves");
        assert_eq!((rows, bytes.len()), (3, 3 * 1024));
        assert_eq!((bytes[200 * 4], bytes[1024 + 200 * 4]), (200, 55), "identity, then turned over");
    }

    #[test]
    fn a_rect_grows_each_way_and_what_is_round_it_comes_in_four_bands() {
        let r = IRect::new(10, 20, 30, 40);
        assert_eq!(grown(r, 5), IRect::new(5, 15, 35, 45));
        assert_eq!(grown(IRect::EMPTY, 5), IRect::EMPTY);
        let bands = around(grown(r, 5), r);
        assert_eq!(bands.len(), 4);
        let area: u32 = bands.iter().map(|b| b.width() * b.height()).sum();
        assert_eq!(area, 30 * 30 - 20 * 20);
        // against an edge, fewer
        assert_eq!(around(IRect::new(0, 0, 35, 45), IRect::new(0, 0, 30, 40)).len(), 2);
        assert!(around(r, r).is_empty());
    }
}
