//! Every shader the app makes, as an OpenGL ES device takes it: translated by naga's GLSL writer
//! for GLSL ES 3.10 and 3.20 (not WebGL), as wgpu's OpenGL backend does for each pipeline's entry
//! point, and its storage buffers counted against what such a device has. A desktop's OpenGL
//! takes a newer GLSL with more in it, so the PC's own OpenGL runs miss what only ES lacks: a
//! PowerVR GE8300 on OpenGL ES 3.2 failed three smudge pipelines over `textureNumLevels`, which no
//! GLSL ES has.

use wgpu::naga;

/// The storage buffers a stage of the PowerVR GE8300's OpenGL ES 3.2 has (Vulkan has 64), the
/// least OpenGL ES 3.1 gives a compute shader.
const STORAGE_BUFFERS_MOST: usize = 8;

/// Each shader module the app makes, as it makes it (label, source): the canvas's
/// (`Pipelines::new`, `FragmentPipelines::new`), the brush store's, 3D painting's, the sphere's
/// and the lighting's.
const SHADERS: &[(&str, &str)] = &[
    ("stamp", concat!(include_str!("shaders/blend.wgsl"), include_str!("shaders/stamp.wgsl"))),
    ("composite", concat!(include_str!("shaders/blend.wgsl"), include_str!("shaders/composite.wgsl"))),
    ("display", include_str!("shaders/display.wgsl")),
    ("surround", include_str!("shaders/surround.wgsl")),
    ("surround picture", include_str!("shaders/surround_picture.wgsl")),
    ("backdrop", include_str!("shaders/backdrop.wgsl")),
    ("transform", include_str!("shaders/transform.wgsl")),
    ("selection", include_str!("shaders/selection.wgsl")),
    ("bounds", include_str!("shaders/bounds.wgsl")),
    ("flip", include_str!("shaders/flip.wgsl")),
    ("crop", include_str!("shaders/crop.wgsl")),
    ("filter", include_str!("shaders/filter.wgsl")),
    ("liquify", include_str!("shaders/liquify.wgsl")),
    (
        "smudge",
        concat!(
            include_str!("shaders/blend.wgsl"),
            include_str!("shaders/smudge.wgsl"),
            include_str!("shaders/smudge_compute.wgsl")
        ),
    ),
    (
        "stamp rule",
        concat!(
            include_str!("shaders/blend.wgsl"),
            include_str!("shaders/stamp.wgsl"),
            include_str!("shaders/stamp_rule.wgsl"),
            include_str!("shaders/stamp_rule_compute.wgsl")
        ),
    ),
    (
        "stamp rule in fragments",
        concat!(
            include_str!("shaders/blend.wgsl"),
            include_str!("shaders/stamp.wgsl"),
            include_str!("shaders/stamp_rule.wgsl"),
            include_str!("shaders/stamp_rule_frag.wgsl")
        ),
    ),
    (
        "smudge in fragments",
        concat!(
            include_str!("shaders/blend.wgsl"),
            include_str!("shaders/smudge.wgsl"),
            include_str!("shaders/smudge_frag.wgsl")
        ),
    ),
    ("overlay", include_str!("shaders/overlay.wgsl")),
    ("store", include_str!("shaders/store.wgsl")),
    ("prefilter", include_str!("shaders/prefilter.wgsl")),
    ("studio", include_str!("shaders/studio.wgsl")),
    ("mesh", include_str!("shaders/mesh.wgsl")),
    ("mesh selection", include_str!("shaders/mesh_selection.wgsl")),
    ("footprint", include_str!("shaders/footprint.wgsl")),
    ("footprint in fragments", include_str!("shaders/footprint_frag.wgsl")),
    ("mips", include_str!("shaders/mips.wgsl")),
    ("project", include_str!("shaders/project.wgsl")),
    ("sphere", include_str!("shaders/sphere.wgsl")),
];

/// What one entry point came to: its module and name, and what failed (none: it translated at
/// every version), with its storage buffers and storage textures.
struct Translated {
    entry: String,
    failed: Vec<String>,
    storage_buffers: usize,
    storage_textures: usize,
}

/// What failed for `label`, before any entry point could be looked at.
fn failed(label: &str, why: String) -> Vec<Translated> {
    vec![Translated { entry: label.to_owned(), failed: vec![why], storage_buffers: 0, storage_textures: 0 }]
}

/// Each entry point of `source` (the module `label`), translated (see [`translate_module`]).
fn translate(label: &str, source: &str) -> Vec<Translated> {
    match naga::front::wgsl::parse_str(source) {
        Ok(module) => translate_module(label, &module),
        Err(e) => failed(label, format!("parsing: {}", e.emit_to_string(source))),
    }
}

/// Each entry point of `module` (`label`), translated for GLSL ES 3.10 and 3.20 as wgpu's OpenGL
/// backend does it: its overrides at their defaults, image loads unchecked (ES has no way to
/// check them).
fn translate_module(label: &str, module: &naga::Module) -> Vec<Translated> {
    use naga::back::glsl;
    let flags = naga::valid::ValidationFlags::all();
    let info = match naga::valid::Validator::new(flags, naga::valid::Capabilities::all()).validate(module) {
        Ok(info) => info,
        Err(e) => return failed(label, format!("validating: {e}")),
    };
    let policies =
        naga::proc::BoundsCheckPolicies { image_load: naga::proc::BoundsCheckPolicy::Unchecked, ..Default::default() };
    let mut out = Vec::new();
    for (index, ep) in module.entry_points.iter().enumerate() {
        let used = info.get_entry_point(index);
        let (mut storage_buffers, mut storage_textures) = (0, 0);
        for (handle, global) in module.global_variables.iter() {
            if used[handle].is_empty() {
                continue;
            }
            if matches!(global.space, naga::AddressSpace::Storage { .. }) {
                storage_buffers += 1;
            }
            if let naga::TypeInner::Image { class: naga::ImageClass::Storage { .. }, .. } =
                module.types[global.ty].inner
            {
                storage_textures += 1;
            }
        }
        let mut why = Vec::new();
        for version in [310, 320] {
            let constants = naga::back::PipelineConstants::default();
            let processed = naga::back::pipeline_constants::process_overrides(
                module,
                &info,
                Some((ep.stage, ep.name.as_str())),
                &constants,
            );
            let (module, info) = match processed {
                Ok(processed) => processed,
                Err(e) => {
                    why.push(format!("GLSL ES {version}: its overrides: {e}"));
                    continue;
                }
            };
            let options = glsl::Options {
                version: glsl::Version::Embedded { version, is_webgl: false },
                writer_flags: glsl::WriterFlags::ADJUST_COORDINATE_SPACE | glsl::WriterFlags::FORCE_POINT_SIZE,
                ..Default::default()
            };
            let pipeline =
                glsl::PipelineOptions { shader_stage: ep.stage, entry_point: ep.name.clone(), multiview: None };
            let mut text = String::new();
            let written = glsl::Writer::new(&mut text, &module, &info, &options, &pipeline, policies)
                .and_then(|mut writer| writer.write());
            if let Err(e) = written {
                why.push(format!("GLSL ES {version}: {e}"));
            }
        }
        out.push(Translated { entry: format!("{label}: {}", ep.name), failed: why, storage_buffers, storage_textures });
    }
    out
}

/// Every shader the app makes translates for GLSL ES 3.10 and 3.20, every entry point of it, and
/// none needs more storage buffers in a stage than an OpenGL ES device has (8). Every shader file
/// is in one of the modules: a new one left out of [`SHADERS`] fails here.
#[test]
fn every_shader_translates_for_opengl_es_3_1_and_3_2() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shaders");
    let mut files = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        // (a plugin's template is joined to its code: see the next test)
        let template = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("plugin_"));
        if path.extension().is_some_and(|e| e == "wgsl") && !template {
            files += 1;
            let text = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
            assert!(
                SHADERS.iter().any(|(_, source)| source.replace("\r\n", "\n").contains(&text)),
                "{} is in none of the shaders checked",
                path.display()
            );
        }
    }
    let all: Vec<Translated> = SHADERS.iter().flat_map(|(label, source)| translate(label, source)).collect();
    let failed: Vec<String> =
        all.iter().flat_map(|t| t.failed.iter().map(move |why| format!("{}: {why}", t.entry))).collect();
    let too_many: Vec<String> = all
        .iter()
        .filter(|t| t.storage_buffers > STORAGE_BUFFERS_MOST)
        .map(|t| format!("{}: {} storage buffers", t.entry, t.storage_buffers))
        .collect();
    let most_buffers = all.iter().max_by_key(|t| t.storage_buffers).map(|t| (t.storage_buffers, t.entry.as_str()));
    let most_textures = all.iter().max_by_key(|t| t.storage_textures).map(|t| (t.storage_textures, t.entry.as_str()));
    println!(
        "{files} shader files in {} modules, {} entry points, {} translations: {} failed; most storage buffers \
         {most_buffers:?}, most storage textures {most_textures:?}",
        SHADERS.len(),
        all.len(),
        all.len() * 2,
        failed.len()
    );
    assert!(failed.is_empty(), "failed for OpenGL ES:\n{}", failed.join("\n"));
    assert!(too_many.is_empty(), "more storage buffers than OpenGL ES has:\n{}", too_many.join("\n"));
}

/// The canvases' pipelines on a device held to the storage buffers an OpenGL ES device has (8 a
/// stage) are made without an error, compute passes and all (a layout asking for more fails).
#[test]
fn the_canvases_pipelines_fit_eight_storage_buffers_a_stage() {
    let Ok(gpu) = crate::Gpu::headless() else { return eprintln!("skipped: no GPU") };
    let limits =
        wgpu::Limits { max_storage_buffers_per_shader_stage: STORAGE_BUFFERS_MOST as u32, ..gpu.adapter.limits() };
    drop(gpu);
    let gpu = crate::Gpu::headless_limited(limits).unwrap();
    assert_eq!(gpu.device.limits().max_storage_buffers_per_shader_stage, STORAGE_BUFFERS_MOST as u32);
    assert!(gpu.computes());
    gpu.prepare_canvases();
    assert_eq!(gpu.errors(), 0);
    assert!(gpu.canvas_pipelines().computes(), "the compute passes gave way to the fragment passes");
}

/// Every plugin template, joined to the least code of its kind, and every bundled plugin's shader
/// code, read as the app reads it, translates for GLSL ES 3.10 and 3.20 as well. (A plugin's
/// shader that fails on a device is refused there alone, never taking the device's backend with
/// it; this keeps the bundled ones drawing on OpenGL ES.)
#[test]
fn every_plugin_shader_translates_for_opengl_es_3_1_and_3_2() {
    let pixel = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> { return colour; }";
    let table = "fn pixel(colour: vec4<f32>, at: vec2<f32>) -> vec4<f32> { return colour * table(0, at.x); }";
    let picture = "fn picture(at: vec2<f32>) -> vec4<f32> { return vec4<f32>(at * picture_size() / 8.0, 0.0, 1.0); }";
    let mut all = Vec::new();
    let templates = [
        ("plugin_filter_1", crate::read_filter(pixel, &[]).map(|c| c.module().clone())),
        ("plugin_filter_layer_17", crate::read_layer_filter(pixel, &[]).map(|c| c.module().clone())),
        ("plugin_filter_layer_18", crate::read_layer_table_filter(table, &[]).map(|c| c.module().clone())),
        ("plugin_picture_2", crate::read_picture(picture).map(|c| c.module().clone())),
        ("plugin_picture_6", crate::read_picture_given(picture, &[]).map(|c| c.module().clone())),
    ];
    for (label, read) in templates {
        match read {
            Ok(module) => all.extend(translate_module(label, &module)),
            Err(why) => all.extend(failed(label, format!("reading: {why}"))),
        }
    }
    // the bundled plugins' code, given the values it reads (`v.<name>`), as the first kind it
    // reads as
    let mut files = Vec::new();
    let mut folders = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins")];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() && !path.ends_with("target") {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "wgsl") {
                files.push(path);
            }
        }
    }
    files.sort();
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        let mut keys: Vec<String> = text
            .match_indices("v.")
            .filter(|(at, _)| {
                text.get(..*at)
                    .and_then(|before| before.chars().next_back())
                    .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_')
            })
            .map(|(at, _)| text[at + 2..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect())
            .filter(|key: &String| !key.is_empty())
            .collect();
        keys.sort();
        keys.dedup();
        let label = path.display().to_string();
        let read = crate::read_layer_table_filter(&text, &keys)
            .or_else(|_| crate::read_layer_filter(&text, &keys))
            .or_else(|_| crate::read_filter(&text, &keys))
            .map(|c| c.module().clone())
            .or_else(|_| crate::read_picture_given(&text, &keys).map(|c| c.module().clone()))
            .or_else(|_| crate::read_picture(&text).map(|c| c.module().clone()));
        match read {
            Ok(module) => all.extend(translate_module(&label, &module)),
            Err(why) => all.extend(failed(&label, format!("reads as no kind of plugin code: {why}"))),
        }
    }
    let failed: Vec<String> =
        all.iter().flat_map(|t| t.failed.iter().map(move |why| format!("{}: {why}", t.entry))).collect();
    println!(
        "5 plugin templates and {} bundled plugin shaders: {} entry points, {} translations, {} failed",
        files.len(),
        all.len(),
        all.len() * 2,
        failed.len()
    );
    assert!(files.len() >= 40, "only {} bundled plugin shaders found", files.len());
    assert!(failed.is_empty(), "failed for OpenGL ES:\n{}", failed.join("\n"));
}
