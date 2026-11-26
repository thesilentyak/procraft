//! Textures on a GPU whose new textures start out stale, and every texture made and written to
//! through here (see [`Gpu::texture`], [`Gpu::copy_texture_to_texture`], [`Gpu::write_texture`]).
//!
//! Intel's Iris Xe graphics on Direct3D 12 get each texture as a resource of its own whose memory
//! isn't cleared first (wgpu makes them so on these GPUs only, gfx-rs/wgpu#10651), and their driver
//! leaves out of a copy into such a texture every block of 4 by 4 texels the copy would write all
//! zero: those blocks keep whatever the memory last held, another texture's pixels. wgpu clears a
//! texture before its first use unless that use writes all of it, as a copy of a whole tile or
//! layer does, and its own clear of a texture it can't draw into is such a copy itself. So boxes
//! of stale colour came out wherever a fresh texture was copied a picture with clear parts into:
//! the layer around strokes, the tiles wet paint keeps, undo steps, and so the layers' and the
//! brushes' pictures (#386). Here, on those GPUs, every texture is made one that can be drawn into
//! and is cleared by drawing as it is made, before anything else reaches it: what a copy then
//! leaves out is clear.
//!
//! Tests on other GPUs can make textures as such a GPU's (see [`Gpu::emulate_stale_textures`]):
//! with the clear, or without it, when a copy into a texture not yet written leaves the blocks it
//! would write all zero holding [`STALE`] instead, as the memory would.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::Gpu;

/// How a device's new textures start out (see the module's note).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaleTextures {
    /// Cleared before they are first read (wgpu's own doing): every GPU but those below.
    Fresh,
    /// Stale until written, so each is made one that can be drawn into and is cleared by drawing
    /// as it is made: Iris Xe on Direct3D 12.
    Cleared,
    /// Stale, and left so (a test's, to see what the clear keeps from happening): a copy into a
    /// texture not yet written leaves the blocks it would write all zero holding [`STALE`].
    Left,
}

impl StaleTextures {
    fn from_u8(v: u8) -> StaleTextures {
        match v {
            1 => StaleTextures::Cleared,
            2 => StaleTextures::Left,
            _ => StaleTextures::Fresh,
        }
    }

    fn as_u8(self) -> u8 {
        match self {
            StaleTextures::Fresh => 0,
            StaleTextures::Cleared => 1,
            StaleTextures::Left => 2,
        }
    }
}

/// What a stale texture's blocks hold where a copy left them out, as a test makes them (see
/// [`Gpu::emulate_stale_textures`]): another texture's colour, as the memory would hold.
pub const STALE: [u8; 4] = [200, 30, 220, 250];

/// [`STALE`] as a colour to clear a texture of a float or (`uint`) unsigned integer format to.
fn stale_colour(uint: bool) -> wgpu::Color {
    let [r, g, b, a] = STALE.map(|c| if uint { f64::from(c) } else { f64::from(c) / 255.0 });
    wgpu::Color { r, g, b, a }
}

/// A layer a copy leaves stale blocks in (see [`Gpu::copy_as_the_driver`]), and whether all of it
/// was stale before the copy.
pub(crate) struct StaleLayer {
    layer: u32,
    all: bool,
}

/// Whether new textures start stale on a GPU (see the module's note): Iris Xe on Direct3D 12, as
/// wgpu tells them (by name, as it does itself).
pub fn starts_stale(info: &wgpu::AdapterInfo) -> bool {
    info.backend == wgpu::Backend::Dx12 && info.name.contains("Iris(R) Xe")
}

/// A part of a texture not yet written, made while textures were made as stale ones without the
/// clear (see [`StaleTextures::Left`]): the texture, its mip level and layer, and whether it was
/// asked for as one to draw into (wgpu then clears it by drawing, before any copy that doesn't
/// write all of it).
struct Unwritten {
    texture: wgpu::Texture,
    mip: u32,
    layer: u32,
    drawn_into: bool,
}

/// A device's textures as stale ones (see the module's note).
pub(crate) struct Stale {
    how: AtomicU8,
    /// Set by a test (see [`Gpu::emulate_stale_textures`]): copies are made as the driver makes
    /// them.
    emulated: std::sync::atomic::AtomicBool,
    unwritten: Mutex<Vec<Unwritten>>,
    /// The pipelines a copy as the driver makes it draws with, by the texture format drawn into.
    copiers: Mutex<Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>>,
}

impl Stale {
    pub(crate) fn new(starts_stale: bool) -> Stale {
        let how = if starts_stale { StaleTextures::Cleared } else { StaleTextures::Fresh };
        Stale {
            how: AtomicU8::new(how.as_u8()),
            emulated: std::sync::atomic::AtomicBool::new(false),
            unwritten: Mutex::new(Vec::new()),
            copiers: Mutex::new(Vec::new()),
        }
    }
}

/// The 4 by 4 blocks of a copy into a stale texture, as its driver writes them (see the module's
/// note): those the copy (in `copied`, from `at` on in the texture drawn into, `size` across)
/// writes all zero keep what the memory held. `{T}` is the texel's type, `{WRITTEN}` whether the
/// texel at `q` has a channel of the format's other than zero (a format's missing ones read as 0
/// or 1).
const COPY_AS_THE_DRIVER: &str = r"
struct Part {
    at: vec2<u32>,
    size: vec2<u32>,
    stale: vec4<f32>,
}

@group(0) @binding(0) var<uniform> part: Part;
@group(0) @binding(1) var copied: texture_2d<{T}>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = array(vec2(-1.0, -3.0), vec2(-1.0, 1.0), vec2(3.0, 1.0));
    return vec4(p[i], 0.0, 1.0);
}

// whether any texel the copy writes into the block of `p` is other than zero
fn block_written(p: vec2<u32>) -> bool {
    let corner = p / 4u * 4u;
    for (var dy = 0u; dy < 4u; dy = dy + 1u) {
        for (var dx = 0u; dx < 4u; dx = dx + 1u) {
            let q = corner + vec2(dx, dy);
            if (all(q >= part.at) && all(q < part.at + part.size)) {
                if ({WRITTEN}) {
                    return true;
                }
            }
        }
    }
    return false;
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<{T}> {
    let p = vec2<u32>(pos.xy);
    if (block_written(p)) {
        return textureLoad(copied, p - part.at, 0);
    }
    return vec4<{T}>(part.stale * {SCALE});
}
";

impl Gpu {
    /// How this device's new textures start out (see the module's note).
    pub fn stale_textures(&self) -> StaleTextures {
        StaleTextures::from_u8(self.stale.how.load(Ordering::Relaxed))
    }

    /// Whether new textures start stale here, so each is cleared as it is made (see the module's
    /// note).
    pub fn textures_start_stale(&self) -> bool {
        self.stale_textures() != StaleTextures::Fresh
    }

    /// Textures made from now on as `how` says, on this device and its clones, copies into them
    /// made as the driver of a GPU whose textures start stale makes them (see the module's note):
    /// [`StaleTextures::Fresh`] as this GPU makes them. For tests on other GPUs.
    #[doc(hidden)]
    pub fn emulate_stale_textures(&self, how: StaleTextures) {
        self.stale.how.store(how.as_u8(), Ordering::Relaxed);
        self.stale.emulated.store(how != StaleTextures::Fresh, Ordering::Relaxed);
        if how == StaleTextures::Fresh {
            self.stale.unwritten.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
        }
    }

    /// Whether a texture as `desc` asks can be cleared by drawing into it: a 2D colour texture of
    /// a format this GPU draws into.
    fn clears_by_drawing(&self, desc: &wgpu::TextureDescriptor<'_>) -> bool {
        let format = desc.format;
        desc.dimension == wgpu::TextureDimension::D2
            && desc.sample_count == 1
            && !format.has_depth_aspect()
            && !format.has_stencil_aspect()
            && self
                .adapter
                .get_texture_format_features(format)
                .allowed_usages
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
    }

    /// A texture made as `desc` says. Where new textures start stale (see the module's note), one
    /// that can also be drawn into, cleared by drawing every mip level and layer of it as it is
    /// made, before any copy or write reaches it.
    pub fn texture(&self, desc: &wgpu::TextureDescriptor<'_>) -> wgpu::Texture {
        let how = self.stale_textures();
        if how == StaleTextures::Fresh {
            return self.device.create_texture(desc);
        }
        let drawn = self.clears_by_drawing(desc);
        let usage = if drawn { desc.usage | wgpu::TextureUsages::RENDER_ATTACHMENT } else { desc.usage };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor { usage, ..desc.clone() });
        match how {
            StaleTextures::Cleared if drawn => self.clear_by_drawing(&texture),
            StaleTextures::Left if self.stale.emulated.load(Ordering::Relaxed) => {
                let drawn_into = desc.usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT);
                let mut unwritten = self.stale.unwritten.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                for mip in 0..desc.mip_level_count {
                    for layer in 0..desc.array_layer_count() {
                        unwritten.push(Unwritten { texture: texture.clone(), mip, layer, drawn_into });
                    }
                }
            }
            _ => {}
        }
        texture
    }

    /// [`Gpu::texture`] holding `data`: each layer's mip levels in turn, each level's rows packed
    /// (as `wgpu::util::DeviceExt::create_texture_with_data` lays them out, layer by layer).
    pub fn texture_with_data(&self, desc: &wgpu::TextureDescriptor<'_>, data: &[u8]) -> wgpu::Texture {
        let desc = wgpu::TextureDescriptor { usage: desc.usage | wgpu::TextureUsages::COPY_DST, ..desc.clone() };
        let texture = self.texture(&desc);
        let format = desc.format;
        let (block_w, block_h) = format.block_dimensions();
        let block = u64::from(format.block_copy_size(None).unwrap_or(4));
        let mut at = 0usize;
        for layer in 0..desc.array_layer_count() {
            for mip in 0..desc.mip_level_count {
                let Some(size) = desc.mip_level_size(mip) else { continue };
                let size = wgpu::Extent3d { depth_or_array_layers: 1, ..size.physical_size(format) };
                let row = u64::from(size.width / block_w) * block;
                let bytes = usize::try_from(row * u64::from(size.height / block_h)).unwrap_or(usize::MAX);
                let Some(level) = data.get(at..at.saturating_add(bytes)) else { return texture };
                self.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: mip,
                        origin: wgpu::Origin3d { x: 0, y: 0, z: layer },
                        aspect: wgpu::TextureAspect::All,
                    },
                    level,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(u32::try_from(row).unwrap_or(u32::MAX)),
                        rows_per_image: None,
                    },
                    size,
                );
                at += bytes;
            }
        }
        texture
    }

    /// Clears every mip level and layer of `texture` by drawing (made able to be drawn into).
    fn clear_by_drawing(&self, texture: &wgpu::Texture) {
        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("texture cleared") });
        for mip in 0..texture.mip_level_count() {
            for layer in 0..texture.depth_or_array_layers() {
                let view = texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("texture cleared"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                });
                drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("texture cleared"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                }));
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// The layers of `into` a copy of `layers` layers to it reaches that are still unwritten and
    /// that it leaves stale blocks in, as the driver of a GPU whose textures start stale copies
    /// (see the module's note): every such layer, the copy writing all of it (`whole`) or wgpu not
    /// clearing it first by drawing (one not asked for as drawn into: wgpu clears it with a copy,
    /// which leaves all of it stale before this one). Those layers are written from now on.
    fn leaves_stale(&self, into: &wgpu::TexelCopyTextureInfo<'_>, layers: u32, whole: bool) -> Vec<StaleLayer> {
        if !self.stale.emulated.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let mut unwritten = self.stale.unwritten.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stale = Vec::new();
        for layer in into.origin.z..into.origin.z.saturating_add(layers) {
            let at = unwritten
                .iter()
                .position(|u| u.texture == *into.texture && u.mip == into.mip_level && u.layer == layer);
            if let Some(i) = at {
                let part = unwritten.swap_remove(i);
                if whole || !part.drawn_into {
                    stale.push(StaleLayer { layer, all: !whole });
                }
            }
        }
        stale
    }

    /// Whether a copy of `size` to `into` writes all of the mip level (each layer it reaches).
    fn writes_whole(into: &wgpu::TexelCopyTextureInfo<'_>, size: wgpu::Extent3d) -> bool {
        let level = into.texture.size().mip_level_size(into.mip_level, into.texture.dimension());
        into.origin.x == 0 && into.origin.y == 0 && size.width >= level.width && size.height >= level.height
    }

    /// `encoder.copy_texture_to_texture`, made as the driver of a GPU whose textures start stale
    /// makes it where a test asks (see [`Gpu::emulate_stale_textures`]).
    pub fn copy_texture_to_texture(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        from: wgpu::TexelCopyTextureInfo<'_>,
        into: wgpu::TexelCopyTextureInfo<'_>,
        size: wgpu::Extent3d,
    ) {
        let stale = self.leaves_stale(&into, size.depth_or_array_layers, Gpu::writes_whole(&into, size));
        if stale.is_empty() {
            encoder.copy_texture_to_texture(from, into, size);
            return;
        }
        let copied = self.copied_texture(into.texture.format(), size);
        encoder.copy_texture_to_texture(from, copied.as_image_copy(), size);
        self.copy_as_the_driver(encoder, &copied, into, size, &stale);
    }

    /// `queue.write_texture`, made as the driver of a GPU whose textures start stale makes it
    /// where a test asks (see [`Gpu::emulate_stale_textures`]).
    pub fn write_texture(
        &self,
        into: wgpu::TexelCopyTextureInfo<'_>,
        data: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) {
        let stale = self.leaves_stale(&into, size.depth_or_array_layers, Gpu::writes_whole(&into, size));
        if stale.is_empty() {
            self.queue.write_texture(into, data, layout, size);
            return;
        }
        let copied = self.copied_texture(into.texture.format(), size);
        self.queue.write_texture(copied.as_image_copy(), data, layout, size);
        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("stale write") });
        self.copy_as_the_driver(&mut encoder, &copied, into, size, &stale);
        self.queue.submit([encoder.finish()]);
    }

    /// The parts of `into` a copy of `size` from a buffer to it leaves stale blocks in (see
    /// [`Gpu::leaves_stale`]); a copy that leaves some goes through `copied` (a texture of its
    /// own) and is drawn on from there.
    pub(crate) fn stale_after_buffer_copy(
        &self,
        into: &wgpu::TexelCopyTextureInfo<'_>,
        size: wgpu::Extent3d,
    ) -> Option<(Vec<StaleLayer>, wgpu::Texture)> {
        let stale = self.leaves_stale(into, size.depth_or_array_layers, Gpu::writes_whole(into, size));
        (!stale.is_empty()).then(|| (stale, self.copied_texture(into.texture.format(), size)))
    }

    /// A texture to put a copy in before it is drawn on as the driver would write it.
    fn copied_texture(&self, format: wgpu::TextureFormat, size: wgpu::Extent3d) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("stale copy"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    /// Draws `copied` (`size`, its layers those of `into` in turn) into `into` as the driver of a
    /// GPU whose textures start stale copies it, on the layers `stale` names: what it would write
    /// all zero in a block, [`STALE`] instead (and all the rest of a layer stale as a whole). The
    /// other layers are copied plainly.
    pub(crate) fn copy_as_the_driver(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        copied: &wgpu::Texture,
        into: wgpu::TexelCopyTextureInfo<'_>,
        size: wgpu::Extent3d,
        stale: &[StaleLayer],
    ) {
        let format = into.texture.format();
        let uint = matches!(format.sample_type(None, None), Some(wgpu::TextureSampleType::Uint));
        let float = matches!(format.sample_type(None, None), Some(wgpu::TextureSampleType::Float { .. }));
        let drawn = into.texture.usage().contains(wgpu::TextureUsages::RENDER_ATTACHMENT);
        for z in 0..size.depth_or_array_layers {
            let layer = into.origin.z + z;
            let from = wgpu::TexelCopyTextureInfo {
                texture: copied,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z },
                aspect: wgpu::TextureAspect::All,
            };
            let one = wgpu::Extent3d { depth_or_array_layers: 1, ..size };
            let to = wgpu::TexelCopyTextureInfo { origin: wgpu::Origin3d { z: layer, ..into.origin }, ..into };
            let Some(this) = stale.iter().find(|s| s.layer == layer).filter(|_| drawn && (uint || float)) else {
                encoder.copy_texture_to_texture(from, to, one);
                continue;
            };
            let pipeline = self.copier(format, uint);
            let part: Vec<u8> = [into.origin.x, into.origin.y, size.width, size.height]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .chain(STALE.iter().flat_map(|&c| (f32::from(c) / 255.0).to_le_bytes()))
                .collect();
            let part = crate::canvas::buffer_init(&self.device, "stale copy", &part, wgpu::BufferUsages::UNIFORM);
            let source = copied.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: z,
                array_layer_count: Some(1),
                ..Default::default()
            });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("stale copy"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: part.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&source) },
                ],
            });
            let target = into.texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_mip_level: into.mip_level,
                mip_level_count: Some(1),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("stale copy"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if this.all { wgpu::LoadOp::Clear(stale_colour(uint)) } else { wgpu::LoadOp::Load },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_scissor_rect(into.origin.x, into.origin.y, size.width, size.height);
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// The pipeline that copies into `format` as the driver of a GPU whose textures start stale
    /// does (texels of unsigned integers: `uint`), made once.
    fn copier(&self, format: wgpu::TextureFormat, uint: bool) -> wgpu::RenderPipeline {
        let mut copiers = self.stale.copiers.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, p)) = copiers.iter().find(|(f, _)| *f == format) {
            return p.clone();
        }
        let written = match format.components() {
            1 => "textureLoad(copied, q - part.at, 0).x != {T}()",
            2 => "any(textureLoad(copied, q - part.at, 0).xy != vec2<{T}>())",
            3 => "any(textureLoad(copied, q - part.at, 0).xyz != vec3<{T}>())",
            _ => "any(textureLoad(copied, q - part.at, 0) != vec4<{T}>())",
        };
        let source = COPY_AS_THE_DRIVER
            .replace("{WRITTEN}", written)
            .replace("{T}", if uint { "u32" } else { "f32" })
            .replace("{SCALE}", if uint { "255.0" } else { "1.0" });
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stale copy"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        // (read with textureLoad only: any float format binds, filterable or not)
        let sample_type =
            if uint { wgpu::TextureSampleType::Uint } else { wgpu::TextureSampleType::Float { filterable: false } };
        let binds = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stale copy"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stale copy"),
            bind_group_layouts: &[Some(&binds)],
            immediate_size: 0,
        });
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stale copy"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        copiers.push((format, pipeline.clone()));
        pipeline
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDE: u32 = 64;

    /// A picture whose left half is clear but for one texel (so one block of it holds paint), its
    /// right half painted.
    fn half_clear() -> Vec<u8> {
        let n = SIDE as usize;
        let mut rgba = vec![0u8; n * n * 4];
        for (i, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (i % n, i / n);
            if x >= n / 2 {
                *px = [(x * 4) as u8, (y * 4) as u8, 90, 255];
            }
        }
        // one texel painted in the block at (8, 8): the rest of that block stays clear
        rgba[(9 * n + 9) * 4..(9 * n + 9) * 4 + 4].copy_from_slice(&[10, 20, 30, 40]);
        rgba
    }

    /// What a picture as [`half_clear`] reads back as, copied into a texture that started stale and
    /// was left so: every block it holds all zero, [`STALE`].
    fn as_left_stale(picture: &[u8]) -> Vec<u8> {
        let n = SIDE as usize;
        let mut out = picture.to_vec();
        for by in 0..n / 4 {
            for bx in 0..n / 4 {
                let texels = || (0..16).map(|k| ((by * 4 + k / 4) * n + bx * 4 + k % 4) * 4);
                if texels().all(|i| picture[i..i + 4] == [0, 0, 0, 0]) {
                    for i in texels() {
                        out[i..i + 4].copy_from_slice(&STALE);
                    }
                }
            }
        }
        out
    }

    fn descriptor(usage: wgpu::TextureUsages) -> wgpu::TextureDescriptor<'static> {
        wgpu::TextureDescriptor {
            label: Some("stale test"),
            size: wgpu::Extent3d { width: SIDE, height: SIDE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: usage | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        }
    }

    /// The ways a picture is put in a texture: written, copied from another texture, copied from a
    /// buffer.
    #[derive(Clone, Copy, Debug)]
    enum Way {
        Written,
        FromTexture,
        FromBuffer,
    }

    /// `picture` put whole into a new texture (`usage` asked for) the way `way` says, and read back.
    fn put(gpu: &Gpu, way: Way, usage: wgpu::TextureUsages, picture: &[u8]) -> Vec<u8> {
        let layout = wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SIDE * 4), rows_per_image: None };
        let size = wgpu::Extent3d { width: SIDE, height: SIDE, depth_or_array_layers: 1 };
        let texture = gpu.texture(&descriptor(usage));
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        match way {
            Way::Written => gpu.write_texture(texture.as_image_copy(), picture, layout, size),
            Way::FromTexture => {
                // (a texture of the GPU's own making: not stale)
                let source = gpu.device.create_texture(&descriptor(wgpu::TextureUsages::empty()));
                gpu.queue.write_texture(source.as_image_copy(), picture, layout, size);
                gpu.copy_texture_to_texture(&mut encoder, source.as_image_copy(), texture.as_image_copy(), size);
            }
            Way::FromBuffer => {
                let usage = wgpu::BufferUsages::COPY_SRC;
                let buffer = crate::canvas::buffer_init(&gpu.device, "stale test", picture, usage);
                let from = wgpu::TexelCopyBufferInfo { buffer: &buffer, layout };
                gpu.copy_buffer_to_texture(&mut encoder, from, texture.as_image_copy(), size);
            }
        }
        gpu.queue.submit([encoder.finish()]);
        crate::read_texture(gpu, &texture, SIDE, SIDE).unwrap()
    }

    #[test]
    fn new_textures_start_stale_only_on_iris_xe_with_direct3d_12() {
        let gpu = |name: &str, backend| wgpu::AdapterInfo {
            name: name.into(),
            ..wgpu::AdapterInfo::new(wgpu::DeviceType::IntegratedGpu, backend)
        };
        assert!(starts_stale(&gpu("Intel(R) Iris(R) Xe Graphics", wgpu::Backend::Dx12)));
        assert!(!starts_stale(&gpu("Intel(R) Iris(R) Xe Graphics", wgpu::Backend::Vulkan)));
        assert!(!starts_stale(&gpu("Intel(R) UHD Graphics 620", wgpu::Backend::Dx12)));
        assert!(!starts_stale(&gpu("NVIDIA GeForce RTX 5060", wgpu::Backend::Dx12)));
    }

    /// A picture with clear parts put whole into a new texture, written or copied from a texture
    /// or a buffer, as on Iris Xe with Direct3D 12, whose driver leaves out the blocks the copy
    /// would write all zero: left stale, those blocks hold what the memory held (as the boxes of
    /// #386 did); with each texture cleared as it is made, the picture comes back exactly as on
    /// any other GPU. A texture to draw into, copied part of a picture, is cleared by wgpu first
    /// and keeps nothing stale; one not to draw into is cleared by wgpu with a copy, which leaves
    /// all of it stale.
    #[test]
    fn a_copy_into_a_new_texture_leaves_nothing_stale_once_textures_are_cleared_as_they_are_made() {
        let Ok(gpu) = Gpu::headless() else { return eprintln!("skipped: no GPU") };
        let picture = half_clear();
        let stale = as_left_stale(&picture);
        let blocks = stale.chunks(4).zip(picture.chunks(4)).filter(|(a, b)| a != b).count() / 16;
        // the left half's blocks but the one painted texel's
        assert_eq!(blocks, (SIDE as usize / 8) * (SIDE as usize / 4) - 1);
        let drawn = wgpu::TextureUsages::RENDER_ATTACHMENT;
        for way in [Way::Written, Way::FromTexture, Way::FromBuffer] {
            for usage in [wgpu::TextureUsages::empty(), wgpu::TextureUsages::TEXTURE_BINDING, drawn] {
                gpu.emulate_stale_textures(StaleTextures::Fresh);
                assert_eq!(put(&gpu, way, usage, &picture), picture, "{way:?} {usage:?} as this GPU makes it");
                gpu.emulate_stale_textures(StaleTextures::Left);
                assert!(put(&gpu, way, usage, &picture) == stale, "{way:?} {usage:?} left stale");
                gpu.emulate_stale_textures(StaleTextures::Cleared);
                assert_eq!(put(&gpu, way, usage, &picture), picture, "{way:?} {usage:?} cleared as made");
            }
        }
        // the top half of the picture into a new texture, left stale: one to draw into is cleared
        // by wgpu first; one not to draw into holds what the memory held below it as well
        let layout = wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(SIDE * 4), rows_per_image: None };
        let part = wgpu::Extent3d { width: SIDE, height: SIDE / 2, depth_or_array_layers: 1 };
        let rows = (SIDE * SIDE / 2 * 4) as usize;
        for (usage, top, below) in [(drawn, &picture, [0; 4]), (wgpu::TextureUsages::TEXTURE_BINDING, &stale, STALE)] {
            gpu.emulate_stale_textures(StaleTextures::Left);
            let texture = gpu.texture(&descriptor(usage));
            gpu.write_texture(texture.as_image_copy(), &picture, layout, part);
            let got = crate::read_texture(&gpu, &texture, SIDE, SIDE).unwrap();
            assert!(got[..rows] == top[..rows], "{usage:?}: the top half");
            assert!(got[rows..].chunks(4).all(|p| p == below), "{usage:?}: below it");
            // cleared as made, nothing stale either way
            gpu.emulate_stale_textures(StaleTextures::Cleared);
            let texture = gpu.texture(&descriptor(usage));
            gpu.write_texture(texture.as_image_copy(), &picture, layout, part);
            let got = crate::read_texture(&gpu, &texture, SIDE, SIDE).unwrap();
            assert_eq!(&got[..rows], &picture[..rows]);
            assert!(got[rows..].iter().all(|&c| c == 0), "{usage:?}: below it, cleared");
        }
        gpu.emulate_stale_textures(StaleTextures::Fresh);
        assert_eq!(gpu.errors(), 0);
    }

    /// Made as a stale GPU's, a texture asked for without being drawn into can be drawn into all
    /// the same (that is how it is cleared), every mip level and layer of an array cleared, and
    /// one made with data holds it; a depth texture is made as asked. As this GPU makes them,
    /// textures are as asked.
    #[test]
    fn textures_made_as_a_stale_gpu_s_are_cleared_by_drawing_every_level_and_layer() {
        let Ok(gpu) = Gpu::headless() else { return eprintln!("skipped: no GPU") };
        let desc = wgpu::TextureDescriptor {
            label: Some("stale array"),
            size: wgpu::Extent3d { width: 32, height: 32, depth_or_array_layers: 3 },
            mip_level_count: 3,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        assert_eq!(gpu.texture(&desc).usage(), desc.usage);
        gpu.emulate_stale_textures(StaleTextures::Cleared);
        assert!(gpu.textures_start_stale());
        let texture = gpu.texture(&desc);
        assert_eq!(texture.usage(), desc.usage | wgpu::TextureUsages::RENDER_ATTACHMENT);
        assert_eq!((texture.mip_level_count(), texture.depth_or_array_layers()), (3, 3));
        let depth = wgpu::TextureDescriptor {
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            mip_level_count: 1,
            size: wgpu::Extent3d { width: 8, height: 8, depth_or_array_layers: 1 },
            ..desc.clone()
        };
        assert_eq!(gpu.texture(&depth).usage(), wgpu::TextureUsages::RENDER_ATTACHMENT);
        // with data: the data, each layer in turn
        let data: Vec<u8> = (0..32 * 32 * 2).map(|i| (i % 7) as u8).collect();
        let two = wgpu::TextureDescriptor {
            mip_level_count: 1,
            size: wgpu::Extent3d { width: 32, height: 32, depth_or_array_layers: 2 },
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            ..desc.clone()
        };
        let with = gpu.texture_with_data(&two, &data);
        gpu.emulate_stale_textures(StaleTextures::Fresh);
        assert!(!gpu.textures_start_stale());
        let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256 * 32 * 2,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            with.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(32) },
            },
            wgpu::Extent3d { width: 32, height: 32, depth_or_array_layers: 2 },
        );
        gpu.queue.submit([encoder.finish()]);
        read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        gpu.wait().unwrap();
        let got = read.slice(..).get_mapped_range().unwrap().to_vec();
        for row in 0..64 {
            assert_eq!(&got[row * 256..row * 256 + 32], &data[row * 32..row * 32 + 32], "row {row}");
        }
        assert_eq!(gpu.errors(), 0);
    }

    /// What clearing each new texture as it is made costs: tiles of a big brush's wet paint (512
    /// pixels a side), made one after another, as a stroke makes them. A few milliseconds a tile
    /// more at most.
    #[test]
    fn clearing_new_textures_as_they_are_made_costs_little() {
        let Ok(gpu) = Gpu::headless() else { return eprintln!("skipped: no GPU") };
        let tile = wgpu::TextureDescriptor {
            label: Some("tile"),
            size: wgpu::Extent3d { width: 512, height: 512, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        let n = 48;
        let mut per_tile = [f64::MAX; 2];
        // (the quickest of a few rounds, each way in turn: other work on the GPU slows some)
        for _ in 0..5 {
            for (i, how) in [StaleTextures::Fresh, StaleTextures::Cleared].into_iter().enumerate() {
                gpu.emulate_stale_textures(how);
                let started = std::time::Instant::now();
                let made: Vec<wgpu::Texture> = (0..n).map(|_| gpu.texture(&tile)).collect();
                gpu.wait().unwrap();
                per_tile[i] = per_tile[i].min(started.elapsed().as_secs_f64() * 1000.0 / f64::from(n));
                drop(made);
            }
        }
        gpu.emulate_stale_textures(StaleTextures::Fresh);
        let more = per_tile[1] - per_tile[0];
        eprintln!(
            "a 512 px tile made in {:.3} ms, cleared as made {:.3} ms ({more:.3} ms more)",
            per_tile[0], per_tile[1]
        );
        assert!(more < 5.0, "clearing a tile as it is made took {more:.3} ms more");
        assert_eq!(gpu.errors(), 0);
    }

    /// The formats the canvas copies besides its layers' (unsigned integers, single floats, half
    /// floats), in an array's second layer: left stale, the blocks a copy writes all zero hold
    /// what the memory held, the rest as copied, the first layer (not written) clear; cleared as
    /// made, all of it as copied.
    #[test]
    fn copies_of_every_kind_of_texel_leave_stale_blocks_only_where_left_stale() {
        let Ok(gpu) = Gpu::headless() else { return eprintln!("skipped: no GPU") };
        let side = 64u32;
        for (format, texel) in [
            (wgpu::TextureFormat::R32Uint, 4u32),
            (wgpu::TextureFormat::R32Float, 4),
            (wgpu::TextureFormat::Rgba16Float, 8),
        ] {
            let row = (side * texel) as usize;
            // the left half zero, the right half bytes of 60 (a texel of each format other than zero)
            let picture: Vec<u8> = (0..row * side as usize).map(|i| if i % row < row / 2 { 0 } else { 60 }).collect();
            let desc = wgpu::TextureDescriptor {
                label: Some("stale formats"),
                size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 2 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            };
            let layout =
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row as u32), rows_per_image: None };
            let size = wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 };
            let mut got = Vec::new();
            for how in [StaleTextures::Left, StaleTextures::Cleared] {
                gpu.emulate_stale_textures(StaleTextures::Fresh);
                gpu.emulate_stale_textures(how);
                let texture = gpu.texture(&wgpu::TextureDescriptor {
                    usage: desc.usage | wgpu::TextureUsages::COPY_DST,
                    ..desc.clone()
                });
                let into = wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: 1 },
                    aspect: wgpu::TextureAspect::All,
                };
                gpu.write_texture(into, &picture, layout, size);
                let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: (row * side as usize * 2) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                encoder.copy_texture_to_buffer(
                    texture.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &read,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(row as u32),
                            rows_per_image: Some(side),
                        },
                    },
                    wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 2 },
                );
                gpu.queue.submit([encoder.finish()]);
                read.slice(..).map_async(wgpu::MapMode::Read, |_| {});
                gpu.wait().unwrap();
                got.push(read.slice(..).get_mapped_range().unwrap().to_vec());
            }
            gpu.emulate_stale_textures(StaleTextures::Fresh);
            let (layer0, layer1) = got[0].split_at(row * side as usize);
            assert!(layer0.iter().all(|&b| b == 0), "{format:?}: the layer not written, left stale");
            let n = texel as usize;
            for (i, (g, p)) in layer1.chunks(n).zip(picture.chunks(n)).enumerate() {
                let left = i * n % row < row / 2;
                assert!(
                    if left { g.iter().any(|&b| b != 0) } else { g == p },
                    "{format:?}: texel {i} left stale: {g:?}"
                );
            }
            assert!(got[1][..row * side as usize].iter().all(|&b| b == 0), "{format:?}: the first layer, cleared");
            assert_eq!(&got[1][row * side as usize..], &picture[..], "{format:?}: cleared as made");
        }
        assert_eq!(gpu.errors(), 0);
    }

    /// Every texture outside the tests, here and in the core, is made through [`Gpu::texture`] (so
    /// it is cleared as it is made where new textures start stale), and copied and written to
    /// through the GPU's own copies (so tests can make them as such a GPU's driver does).
    #[test]
    fn every_texture_outside_the_tests_is_made_and_written_through_the_gpu() {
        fn sources(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    sources(&path, found);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    found.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        sources(&root.join("src"), &mut files);
        sources(&root.join("../core/src"), &mut files);
        assert!(files.len() > 40, "the sources found: {}", files.len());
        let raw = [
            "create_texture(",
            "create_texture_with_data(",
            "encoder.copy_texture_to_texture(",
            "queue.write_texture(",
            "encoder.copy_buffer_to_texture(",
        ];
        let mut found = Vec::new();
        for path in files {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_owned();
            // (the core's files of tests, and the copies themselves)
            if name.ends_with("tests.rs") || name == "stale.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let code = text
                .split(
                    "
mod tests {",
                )
                .next()
                .unwrap_or(&text);
            for (i, line) in code.lines().enumerate() {
                if line.trim_start().starts_with("//") {
                    continue;
                }
                for pattern in raw {
                    let placed_own = name == "placed.rs" && pattern == "encoder.copy_buffer_to_texture(";
                    if line.contains(pattern) && !placed_own {
                        found.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
                    }
                }
            }
        }
        assert!(found.is_empty(), "made or written to around the GPU's own: {found:#?}");
    }
}
