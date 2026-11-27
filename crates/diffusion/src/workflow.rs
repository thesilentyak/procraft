//! Workflows made in ComfyUI and run from the app: a graph in ComfyUI's API format with the
//! tooling add-on's marker nodes, which say where the app's inputs go and where results come out.
//! The markers are the ones krita-ai-diffusion's custom graphs use, so its workflows run here as
//! they are (we follow the add-on's documented behaviour; none of either project's code is used).
//!
//! - **Krita Canvas**: the visible picture, its width and height, the seed, and the selection as a
//!   mask.
//! - **Krita Selection**: the selection as a mask, whether there is one, and the area's offset.
//! - **Parameter**: a value the app sets (a prompt, text, a number, a switch, a choice).
//! - **Krita Image Layer** and **Krita Mask Layer**: a layer of the artwork, picked in the app.
//! - **Krita Output**: what reaches it comes back to the app as results.
//!
//! [`Workflow::prepare`] swaps each marker for what the server can run: pictures and masks for the
//! add-on's base64 loaders, values written straight into the inputs that used a marker's output,
//! and Krita Output for the image-sending node the worker listens to.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::comfy::input_choices;
use crate::{Error, Graph, Picture};

const CANVAS: &str = "ETN_KritaCanvas";
const SELECTION: &str = "ETN_KritaSelection";
const PARAMETER: &str = "ETN_Parameter";
const IMAGE_LAYER: &str = "ETN_KritaImageLayer";
const MASK_LAYER: &str = "ETN_KritaMaskLayer";
const OUTPUT: &str = "ETN_KritaOutput";

/// A Parameter node's min or max at or beyond this means "not set" (the node's defaults are ±1e10).
const UNSET: f64 = 1e9;

/// A workflow: a name and its graph, renumbered from 1 (see [`Graph::from_api`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Workflow {
    pub name: String,
    graph: Graph,
}

/// What a parameter asks for, and so which control the app shows for it.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    /// The prompt: a field with tag suggestions.
    Prompt,
    /// A negative prompt: a plain text field.
    NegativePrompt,
    Text,
    /// A number; `min` and `max` when both are known (a slider), whole numbers when `integer`.
    Number {
        min: Option<f64>,
        max: Option<f64>,
        integer: bool,
    },
    Toggle,
    /// One of a list, from the input it feeds.
    Choice(Vec<String>),
    /// A layer of the artwork as a picture.
    ImageLayer,
    /// A layer of the artwork as a mask (its alpha).
    MaskLayer,
}

/// A value the app sets, shown as one control.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    /// Its label, and its key: values are set and remembered by name. A name used twice gets
    /// " (2)", " (3)"... after the first.
    pub name: String,
    pub kind: ParamKind,
    /// What the control starts at, as the node gives it (`"true"`, `"0.75"`, a choice).
    pub default: String,
}

/// A parameter's value, as the app's control has it.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    Text(String),
    Number(f64),
    Toggle(bool),
}

/// What one run of a workflow gets from the app.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowInputs {
    /// The visible picture, the whole canvas (straight RGBA).
    pub canvas: Picture,
    /// The selection over the whole canvas (one byte per pixel, white selected), if any.
    pub selection: Option<Vec<u8>>,
    pub seed: u64,
    /// The parameters' values, by name; one not given keeps its default.
    pub values: BTreeMap<String, ParamValue>,
    /// The picked layers at canvas size, by the layer parameter's name; one not given is the
    /// canvas picture.
    pub layers: BTreeMap<String, Picture>,
}

/// Where an Output node's pictures go.
#[derive(Clone, Debug, PartialEq)]
pub struct OutputPlace {
    /// The node's id in the prepared graph, to tell which pictures it sent.
    pub node: String,
    /// The layer name it asks for (empty: the app names it).
    pub name: String,
    /// The offset from the canvas's top left.
    pub x: i64,
    pub y: i64,
    /// Its batch mode is "animation": the pictures it sends are the frames of one animation.
    pub animation: bool,
}

/// A workflow ready to queue.
#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    pub graph: Graph,
    /// One entry per Output node, in node order: the pictures sent come in that order.
    pub outputs: Vec<OutputPlace>,
    /// The part of the canvas the workflow got as its canvas: x, y, width, height.
    pub area: [u32; 4],
}

impl Workflow {
    /// The workflow in `text`: ComfyUI's API format (as "Export (API)" saves it, or as the add-on
    /// sends it), or that inside `{"prompt": ...}`. Refuses ComfyUI's regular save format, and a
    /// workflow without a Krita Output node, saying what to do.
    pub fn parse(name: &str, text: &str) -> Result<Workflow, Error> {
        let value: Value =
            serde_json::from_str(text).map_err(|e| request(format!("this isn't a workflow file ({e})")))?;
        Workflow::from_value(name, &value)
    }

    /// The workflow in `value`, as [`parse`](Self::parse) reads it.
    pub fn from_value(name: &str, value: &Value) -> Result<Workflow, Error> {
        let nodes = value.get("prompt").unwrap_or(value);
        if nodes.get("nodes").is_some() && nodes.get("links").is_some() {
            return Err(request("this is ComfyUI's regular save format: in ComfyUI use Workflow › Export (API)"));
        }
        let nodes = nodes.as_object().ok_or_else(|| request("this isn't a workflow file"))?;
        let graph = Graph::from_api(nodes).map_err(request)?;
        if graph.nodes_of(OUTPUT).is_empty() {
            return Err(request("the workflow has no Krita Output node"));
        }
        Ok(Workflow { name: name.to_owned(), graph })
    }

    /// The graph as given (renumbered), to keep or export it.
    pub fn to_json(&self) -> Value {
        self.graph.to_json()
    }

    /// The node classes whose inputs [`parameters`](Self::parameters) needs described: the ones
    /// an "auto" or "choice" Parameter feeds.
    /// Whether `info` (the server's `/object_info`) describes every node this workflow uses.
    pub fn described_by(&self, info: &Map<String, Value>) -> bool {
        self.graph.classes().all(|class| info.contains_key(class))
    }

    pub fn classes_to_describe(&self) -> Vec<String> {
        let mut classes = Vec::new();
        for node in self.graph.nodes_of(PARAMETER) {
            if let Some((consumer, _)) = self.graph.consumers(&node, 0).first()
                && let Some(class) = self.graph.class(consumer)
                && !classes.iter().any(|c| c == class)
            {
                classes.push(class.to_owned());
            }
        }
        classes
    }

    /// The values the app sets: one per Parameter, Image Layer and Mask Layer node, sorted by
    /// name. `info` holds the server's description of the classes
    /// [`classes_to_describe`](Self::classes_to_describe) named (`/object_info`), keyed by class;
    /// one missing makes its parameters plain text.
    pub fn parameters(&self, info: &Map<String, Value>) -> Vec<Param> {
        let mut params = Vec::new();
        for (class, kind_of) in
            [(PARAMETER, None), (IMAGE_LAYER, Some(ParamKind::ImageLayer)), (MASK_LAYER, Some(ParamKind::MaskLayer))]
        {
            for node in self.graph.nodes_of(class) {
                let Some(inputs) = self.graph.inputs(&node) else { continue };
                let name = text_input(inputs, "name").unwrap_or_else(|| "Parameter".to_owned());
                let default = match inputs.get("default") {
                    Some(Value::String(text)) => text.clone(),
                    Some(Value::Null) | None => String::new(),
                    Some(other) => other.to_string(),
                };
                let kind = match &kind_of {
                    Some(kind) => kind.clone(),
                    None => self.parameter_kind(&node, inputs, info),
                };
                params.push(Param { name, kind, default });
            }
        }
        params.sort_by_key(|a| a.name.to_lowercase());
        let mut seen: BTreeMap<String, u32> = BTreeMap::new();
        for param in &mut params {
            let count = seen.entry(param.name.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                param.name = format!("{} ({count})", param.name);
            }
        }
        params
    }

    fn parameter_kind(&self, node: &str, inputs: &Map<String, Value>, info: &Map<String, Value>) -> ParamKind {
        let number = |integer: bool, fed: Option<&Value>| {
            let bound = |key: &str| {
                inputs
                    .get(key)
                    .and_then(Value::as_f64)
                    .filter(|v| v.abs() < UNSET)
                    .or_else(|| fed.and_then(|spec| spec.pointer(&format!("/1/{key}"))).and_then(Value::as_f64))
                    .filter(|v| v.abs() < UNSET)
            };
            let (min, max) = (bound("min"), bound("max"));
            if min.is_some() && max.is_some() {
                ParamKind::Number { min, max, integer }
            } else {
                ParamKind::Number { min: None, max: None, integer }
            }
        };
        let fed = self.graph.consumers(node, 0).into_iter().next().and_then(|(consumer, input)| {
            let class = self.graph.class(&consumer)?;
            let described = info.get(class)?;
            Some((described.clone(), input))
        });
        let spec = fed.as_ref().and_then(|(described, input)| input_spec(described, input));
        let choices = || fed.as_ref().map(|(described, input)| choices_of(described, input)).unwrap_or_default();
        match text_input(inputs, "type").as_deref().unwrap_or("auto") {
            "prompt (positive)" => ParamKind::Prompt,
            "prompt (negative)" => ParamKind::NegativePrompt,
            "text" => ParamKind::Text,
            "number" => number(false, spec),
            "number (integer)" => number(true, spec),
            "toggle" => ParamKind::Toggle,
            "choice" => ParamKind::Choice(choices()),
            _ => {
                let list = choices();
                if !list.is_empty() {
                    return ParamKind::Choice(list);
                }
                match spec.and_then(|s| s.get(0)).and_then(Value::as_str) {
                    Some("INT") => number(true, spec),
                    Some("FLOAT") => number(false, spec),
                    Some("BOOLEAN") => ParamKind::Toggle,
                    Some("STRING") if spec.and_then(|s| s.pointer("/1/multiline")) == Some(&Value::Bool(true)) => {
                        let name = text_input(inputs, "name").unwrap_or_default().to_lowercase();
                        if name.contains("neg") { ParamKind::NegativePrompt } else { ParamKind::Prompt }
                    }
                    _ => ParamKind::Text,
                }
            }
        }
    }

    /// The graph to queue for `inputs`: every marker swapped for what the server runs (see the
    /// module's notes). `params` are this workflow's [`parameters`](Self::parameters), for their
    /// kinds and defaults. `info` is the server's `/object_info` (empty if unknown): the seed is
    /// fitted to each node's range there.
    pub fn prepare(
        &self,
        params: &[Param],
        inputs: &WorkflowInputs,
        info: &Map<String, Value>,
    ) -> Result<Prepared, Error> {
        let mut g = self.graph.clone();
        let canvas_size = (inputs.canvas.width, inputs.canvas.height);
        let selection = inputs.selection.as_deref().filter(|mask| mask.iter().any(|&v| v > 0));
        let area = self.area(selection, canvas_size);
        let [ax, ay, aw, ah] = area;
        let picture = inputs.canvas.cropped(ax, ay, aw, ah)?;
        let mask = match selection {
            Some(mask) => crop_mask(mask, canvas_size.0, area)?,
            None => vec![255; (aw as usize).saturating_mul(ah as usize)],
        };
        let mask_png = || -> Result<Value, Error> { Ok(json!(base64(&Picture::mask_png(aw, ah, &mask)?))) };

        for node in g.nodes_of(CANVAS) {
            g.relink(&node, 1, &json!(aw));
            g.relink(&node, 2, &json!(ah));
            for (consumer, input) in g.consumers(&node, 3) {
                let spec = g.class(&consumer).and_then(|class| info.get(class)).and_then(|i| input_spec(i, &input));
                g.set(&consumer, &input, json!(fit_seed(inputs.seed, spec)));
            }
            if !g.consumers(&node, 4).is_empty() {
                let loader = g.add("ETN_LoadMaskBase64", [("mask", mask_png()?)]);
                g.relink(&node, 4, &loader.out(0).into());
            }
            g.replace(&node, "ETN_LoadImageBase64", [("image", json!(base64(&picture.to_png()?)))]);
        }
        for node in g.nodes_of(SELECTION) {
            g.relink(&node, 1, &json!(selection.is_some()));
            g.relink(&node, 2, &json!(ax));
            g.relink(&node, 3, &json!(ay));
            g.replace(&node, "ETN_LoadMaskBase64", [("mask", mask_png()?)]);
        }

        let mut by_name: BTreeMap<String, &Param> = BTreeMap::new();
        for param in params {
            by_name.insert(param.name.clone(), param);
        }
        let mut seen: BTreeMap<String, u32> = BTreeMap::new();
        let mut layer_nodes = Vec::new();
        let mut marked = Vec::new();
        for class in [PARAMETER, IMAGE_LAYER, MASK_LAYER] {
            for node in g.nodes_of(class) {
                let base =
                    g.inputs(&node).and_then(|i| text_input(i, "name")).unwrap_or_else(|| "Parameter".to_owned());
                marked.push((base, node, class));
            }
        }
        // the same naming as `parameters`: sorted by name, a repeated name numbered
        marked.sort_by_key(|a| a.0.to_lowercase());
        for (base, node, class) in marked {
            let count = seen.entry(base.clone()).or_insert(0);
            *count += 1;
            let name = if *count > 1 { format!("{base} ({count})") } else { base };
            if class == PARAMETER {
                let Some(param) = by_name.get(&name) else { continue };
                let value = parameter_value(param, inputs.values.get(&name));
                g.relink(&node, 0, &value);
            } else {
                layer_nodes.push((node, class, name));
            }
        }
        for (node, class, name) in layer_nodes {
            let layer = inputs.layers.get(&name).unwrap_or(&inputs.canvas);
            let layer = layer.resized(canvas_size.0, canvas_size.1)?.cropped(ax, ay, aw, ah)?;
            let alpha: Vec<u8> = layer.rgba.chunks_exact(4).map(|p| p.get(3).copied().unwrap_or(255)).collect();
            let alpha_png = json!(base64(&Picture::mask_png(aw, ah, &alpha)?));
            if class == IMAGE_LAYER {
                if !g.consumers(&node, 1).is_empty() {
                    let loader = g.add("ETN_LoadMaskBase64", [("mask", alpha_png)]);
                    g.relink(&node, 1, &loader.out(0).into());
                }
                g.replace(&node, "ETN_LoadImageBase64", [("image", json!(base64(&layer.to_png()?)))]);
            } else {
                g.replace(&node, "ETN_LoadMaskBase64", [("mask", alpha_png)]);
            }
        }

        let mut outputs = Vec::new();
        for node in g.nodes_of(OUTPUT) {
            let Some(found) = g.inputs(&node) else { continue };
            let offset = |key: &str| found.get(key).and_then(Value::as_i64).unwrap_or(0);
            outputs.push(OutputPlace {
                node: node.clone(),
                name: text_input(found, "name").unwrap_or_default(),
                x: i64::from(ax) + offset("x"),
                y: i64::from(ay) + offset("y"),
                animation: text_input(found, "batch_mode").as_deref() == Some("animation"),
            });
            let images = found.get("images").cloned().unwrap_or(Value::Null);
            g.replace(&node, "ETN_SendImageWebSocket", [("images", images), ("format", json!("PNG"))]);
        }
        Ok(Prepared { graph: g, outputs, area })
    }

    /// The part of the canvas the workflow works on, `[x, y, width, height]`: the whole canvas
    /// (`width × height`), or with a `selection` (one byte per canvas pixel) and a Krita
    /// Selection node not set to "entire image", the selection's bounds and the node's padding.
    pub fn area(&self, selection: Option<&[u8]>, (width, height): (u32, u32)) -> [u32; 4] {
        let whole = [0, 0, width, height];
        let Some(mask) = selection else { return whole };
        let Some(node) = self.graph.nodes_of(SELECTION).into_iter().next() else { return whole };
        let inputs = self.graph.inputs(&node);
        if inputs.and_then(|i| text_input(i, "context")).as_deref() == Some("entire image") {
            return whole;
        }
        let padding = inputs.and_then(|i| i.get("padding")).and_then(Value::as_u64).unwrap_or(0);
        let padding = u32::try_from(padding).unwrap_or(u32::MAX);
        let Some([x0, y0, x1, y1]) = bounds(mask, width) else { return whole };
        let left = x0.saturating_sub(padding);
        let top = y0.saturating_sub(padding);
        let right = x1.saturating_add(padding).min(width);
        let bottom = y1.saturating_add(padding).min(height);
        [left, top, right.saturating_sub(left).max(1), bottom.saturating_sub(top).max(1)]
    }
}

/// The value written where a parameter was linked: typed as its kind wants, from `given` or else
/// its default.
fn parameter_value(param: &Param, given: Option<&ParamValue>) -> Value {
    match &param.kind {
        ParamKind::Number { integer, .. } => {
            let number = match given {
                Some(ParamValue::Number(n)) => *n,
                Some(ParamValue::Text(t)) => t.trim().parse().unwrap_or(0.0),
                Some(ParamValue::Toggle(b)) => f64::from(u8::from(*b)),
                None => param.default.trim().parse().unwrap_or(0.0),
            };
            if *integer { json!(number.round() as i64) } else { json!(number) }
        }
        ParamKind::Toggle => {
            let on = match given {
                Some(ParamValue::Toggle(b)) => *b,
                Some(ParamValue::Number(n)) => *n != 0.0,
                Some(ParamValue::Text(t)) => is_true(t),
                None => is_true(&param.default),
            };
            json!(on)
        }
        ParamKind::Choice(list) => {
            let chosen = match given {
                Some(ParamValue::Text(t)) => t.clone(),
                _ => param.default.clone(),
            };
            if chosen.is_empty() { json!(list.first().cloned().unwrap_or_default()) } else { json!(chosen) }
        }
        _ => match given {
            Some(ParamValue::Text(t)) => json!(t),
            Some(ParamValue::Number(n)) => json!(n.to_string()),
            Some(ParamValue::Toggle(b)) => json!(b.to_string()),
            None => json!(param.default),
        },
    }
}

fn is_true(text: &str) -> bool {
    matches!(text.trim().to_lowercase().as_str(), "true" | "1" | "yes" | "on")
}

fn text_input(inputs: &Map<String, Value>, key: &str) -> Option<String> {
    inputs.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// An input's description in a node's `/object_info`: `[type or list, {options}]`.
fn input_spec<'a>(info: &'a Value, input: &str) -> Option<&'a Value> {
    info.pointer(&format!("/input/required/{input}")).or_else(|| info.pointer(&format!("/input/optional/{input}")))
}

/// `seed` within an integer input's range, as `spec` (`[type, {min, max}]`) gives it: a seed past
/// the end wraps round, so seeds one apart stay apart. Some nodes take seeds only up to 2³¹ - 1.
fn fit_seed(seed: u64, spec: Option<&Value>) -> u64 {
    let options = spec.and_then(|s| s.get(1));
    let bound = |key: &str| options.and_then(|o| o.get(key)).and_then(Value::as_f64);
    let min = bound("min").map_or(0, |m| m.max(0.0) as u64);
    match bound("max") {
        Some(max) if max >= 0.0 && (max as u64) >= min && seed > max as u64 => {
            let span = (max as u64 - min).saturating_add(1);
            min + seed % span
        }
        _ => seed,
    }
}

/// The choices an input offers, required or optional.
fn choices_of(info: &Value, input: &str) -> Vec<String> {
    let required = input_choices(info, input);
    if !required.is_empty() {
        return required;
    }
    let optional = json!({ "input": { "required": info.pointer("/input/optional").cloned().unwrap_or(json!({})) } });
    input_choices(&optional, input)
}

/// The box round a mask's selected pixels: left, top, right, bottom (exclusive).
fn bounds(mask: &[u8], width: u32) -> Option<[u32; 4]> {
    if width == 0 {
        return None;
    }
    let mut found: Option<[u32; 4]> = None;
    for (i, &v) in mask.iter().enumerate() {
        if v == 0 {
            continue;
        }
        let i = u32::try_from(i).ok()?;
        let (x, y) = (i % width, i / width);
        found = Some(match found {
            None => [x, y, x + 1, y + 1],
            Some([l, t, r, b]) => [l.min(x), t.min(y), r.max(x + 1), b.max(y + 1)],
        });
    }
    found
}

/// The part of a canvas-wide mask in `area`.
fn crop_mask(mask: &[u8], width: u32, [x, y, w, h]: [u32; 4]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity((w as usize).saturating_mul(h as usize));
    for row in y..y.saturating_add(h) {
        let start = (row as usize).saturating_mul(width as usize).saturating_add(x as usize);
        let line = mask
            .get(start..start.saturating_add(w as usize))
            .ok_or_else(|| Error::picture("the selection doesn't match the canvas"))?;
        out.extend_from_slice(line);
    }
    Ok(out)
}

fn base64(bytes: &[u8]) -> String {
    procraft_engine::base64::encode(bytes)
}

fn request(reason: impl std::fmt::Display) -> Error {
    Error::Request { reason: reason.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_to_image() -> Workflow {
        let text = r#"{
            "3": {"class_type": "KSampler", "inputs": {"model": ["4", 0], "seed": ["12", 3], "steps": ["20", 0],
                  "cfg": ["21", 0], "sampler_name": ["22", 0], "positive": ["6", 0], "negative": ["7", 0],
                  "latent_image": ["5", 0], "scheduler": "simple", "denoise": 1.0}},
            "4": {"class_type": "UNETLoader", "inputs": {"unet_name": "z_image_turbo_bf16.safetensors", "weight_dtype": "default"}},
            "5": {"class_type": "EmptySD3LatentImage", "inputs": {"width": ["12", 1], "height": ["12", 2], "batch_size": 1}},
            "6": {"class_type": "CLIPTextEncode", "inputs": {"text": ["11", 0], "clip": ["4", 0]}},
            "7": {"class_type": "CLIPTextEncode", "inputs": {"text": ["13", 0], "clip": ["4", 0]}},
            "8": {"class_type": "VAEDecode", "inputs": {"samples": ["3", 0], "vae": ["4", 0]}},
            "9": {"class_type": "ETN_KritaOutput", "inputs": {"images": ["8", 0], "x": 0, "y": 0, "name": "Koi", "batch_mode": "default", "resize_canvas": false}},
            "11": {"class_type": "ETN_Parameter", "inputs": {"name": "Prompt", "type": "prompt (positive)", "default": "a pond"}},
            "12": {"class_type": "ETN_KritaCanvas", "inputs": {}},
            "13": {"class_type": "ETN_Parameter", "inputs": {"name": "Negative", "type": "auto", "default": ""}},
            "20": {"class_type": "ETN_Parameter", "inputs": {"name": "Steps", "type": "auto", "default": "8", "min": -1e10, "max": 1e10}},
            "21": {"class_type": "ETN_Parameter", "inputs": {"name": "Detail", "type": "number", "default": "1.5", "min": 1, "max": 3}},
            "22": {"class_type": "ETN_Parameter", "inputs": {"name": "Sampler", "type": "choice", "default": ""}},
            "23": {"class_type": "ETN_Parameter", "inputs": {"name": "Upscale", "type": "toggle", "default": "false"}},
            "24": {"class_type": "ImageScaleBy", "inputs": {"image": ["8", 0], "upscale_method": "lanczos", "scale_by": 2.0, "enabled": ["23", 0]}}
        }"#;
        Workflow::parse("Z-Image", text).unwrap()
    }

    fn info() -> Map<String, Value> {
        json!({
            "KSampler": {"input": {"required": {
                "steps": ["INT", {"default": 20, "min": 1, "max": 10000}],
                "cfg": ["FLOAT", {"default": 8.0, "min": 0.0, "max": 100.0}],
                "sampler_name": ["COMBO", {"options": ["euler", "dpmpp_2m"]}],
            }}},
            "CLIPTextEncode": {"input": {"required": {"text": ["STRING", {"multiline": true}]}}},
            "ImageScaleBy": {"input": {"required": {"enabled": ["BOOLEAN", {}]}}},
        })
        .as_object()
        .cloned()
        .unwrap()
    }

    fn picture(width: u32, height: u32) -> Picture {
        Picture::new(width, height, vec![200; (width * height * 4) as usize]).unwrap()
    }

    fn inputs(values: &[(&str, ParamValue)]) -> WorkflowInputs {
        WorkflowInputs {
            canvas: picture(64, 48),
            selection: None,
            seed: 7,
            values: values.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect(),
            layers: BTreeMap::new(),
        }
    }

    fn input(prepared: &Prepared, class: &str, name: &str) -> Value {
        let node = prepared.graph.nodes_of(class).into_iter().next().unwrap();
        prepared.graph.inputs(&node).unwrap()[name].clone()
    }

    #[test]
    fn the_regular_save_format_and_missing_outputs_are_explained() {
        let ui = r#"{"nodes": [], "links": [], "version": 0.4}"#;
        assert!(Workflow::parse("x", ui).unwrap_err().to_string().contains("Export (API)"));
        let no_output = r#"{"1": {"class_type": "KSampler", "inputs": {}}}"#;
        assert!(Workflow::parse("x", no_output).unwrap_err().to_string().contains("Krita Output"));
        assert!(Workflow::parse("x", "not json").is_err());
        let wrapped = r#"{"prompt": {"1": {"class_type": "ETN_KritaOutput", "inputs": {}}}}"#;
        assert!(Workflow::parse("x", wrapped).is_ok());
    }

    #[test]
    fn parameters_get_their_kinds_sorted_by_name() {
        let w = text_to_image();
        let mut classes = w.classes_to_describe();
        classes.sort();
        assert_eq!(classes, vec!["CLIPTextEncode", "ImageScaleBy", "KSampler"]);
        let params = w.parameters(&info());
        let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Detail", "Negative", "Prompt", "Sampler", "Steps", "Upscale"]);
        let kind = |name: &str| params.iter().find(|p| p.name == name).unwrap().kind.clone();
        assert_eq!(kind("Prompt"), ParamKind::Prompt);
        assert_eq!(kind("Negative"), ParamKind::NegativePrompt);
        assert_eq!(kind("Steps"), ParamKind::Number { min: Some(1.0), max: Some(10000.0), integer: true });
        assert_eq!(kind("Detail"), ParamKind::Number { min: Some(1.0), max: Some(3.0), integer: false });
        assert_eq!(kind("Sampler"), ParamKind::Choice(vec!["euler".into(), "dpmpp_2m".into()]));
        assert_eq!(kind("Upscale"), ParamKind::Toggle);
        // without the server's descriptions an auto parameter is text
        assert_eq!(w.parameters(&Map::new()).iter().find(|p| p.name == "Steps").unwrap().kind, ParamKind::Text);
    }

    #[test]
    fn markers_become_what_the_server_runs() {
        let w = text_to_image();
        let params = w.parameters(&info());
        let prepared = w
            .prepare(
                &params,
                &inputs(&[("Prompt", ParamValue::Text("koi pond, scenery, no humans".into()))]),
                &Map::new(),
            )
            .unwrap();
        let g = &prepared.graph;
        assert!(g.nodes_of(CANVAS).is_empty() && g.nodes_of(OUTPUT).is_empty());
        assert_eq!(g.nodes_of("ETN_SendImageWebSocket").len(), 1);
        assert_eq!(input(&prepared, "EmptySD3LatentImage", "width"), 64);
        assert_eq!(input(&prepared, "EmptySD3LatentImage", "height"), 48);
        assert_eq!(input(&prepared, "KSampler", "seed"), 7);
        assert_eq!(input(&prepared, "KSampler", "steps"), 8);
        assert_eq!(input(&prepared, "KSampler", "cfg"), 1.5);
        assert_eq!(input(&prepared, "KSampler", "sampler_name"), "euler");
        assert_eq!(input(&prepared, "ImageScaleBy", "enabled"), false);
        let texts: Vec<Value> =
            g.nodes_of("CLIPTextEncode").iter().map(|n| g.inputs(n).unwrap()["text"].clone()).collect();
        assert!(texts.contains(&json!("koi pond, scenery, no humans")));
        assert!(texts.contains(&json!("")));
        let sender = prepared.graph.nodes_of("ETN_SendImageWebSocket");
        assert_eq!(
            prepared.outputs,
            vec![OutputPlace { node: sender[0].clone(), name: "Koi".into(), x: 0, y: 0, animation: false }]
        );
        assert_eq!(prepared.area, [0, 0, 64, 48]);
    }

    #[test]
    fn given_values_are_typed_as_their_parameter() {
        let w = text_to_image();
        let params = w.parameters(&info());
        let prepared = w
            .prepare(
                &params,
                &inputs(&[
                    ("Steps", ParamValue::Number(11.6)),
                    ("Upscale", ParamValue::Toggle(true)),
                    ("Sampler", ParamValue::Text("dpmpp_2m".into())),
                ]),
                &Map::new(),
            )
            .unwrap();
        assert_eq!(input(&prepared, "KSampler", "steps"), 12);
        assert_eq!(input(&prepared, "ImageScaleBy", "enabled"), true);
        assert_eq!(input(&prepared, "KSampler", "sampler_name"), "dpmpp_2m");
    }

    #[test]
    fn a_selection_node_narrows_the_area_and_places_the_output() {
        let text = r#"{
            "1": {"class_type": "ETN_KritaCanvas", "inputs": {}},
            "2": {"class_type": "ETN_KritaSelection", "inputs": {"context": "automatic", "padding": 2}},
            "3": {"class_type": "Inpaint", "inputs": {"image": ["1", 0], "mask": ["2", 0], "w": ["1", 1], "active": ["2", 1]}},
            "4": {"class_type": "ETN_KritaOutput", "inputs": {"images": ["3", 0], "x": 0, "y": 0, "name": ""}},
            "5": {"class_type": "ETN_KritaImageLayer", "inputs": {"name": "Reference"}},
            "6": {"class_type": "Use", "inputs": {"image": ["5", 0], "mask": ["5", 1]}}
        }"#;
        let w = Workflow::parse("Fill", text).unwrap();
        let params = w.parameters(&Map::new());
        assert_eq!(
            params,
            vec![Param { name: "Reference".into(), kind: ParamKind::ImageLayer, default: String::new() }]
        );
        let mut mask = vec![0u8; 64 * 48];
        for y in 10..20 {
            for x in 30..40 {
                mask[y * 64 + x] = 255;
            }
        }
        let mut run = inputs(&[]);
        run.selection = Some(mask);
        let prepared = w.prepare(&params, &run, &Map::new()).unwrap();
        assert_eq!(prepared.area, [28, 8, 14, 14]);
        assert_eq!(
            prepared.outputs,
            vec![OutputPlace { node: "4".into(), name: String::new(), x: 28, y: 8, animation: false }]
        );
        assert_eq!(input(&prepared, "Inpaint", "w"), 14);
        assert_eq!(input(&prepared, "Inpaint", "active"), true);
        let g = &prepared.graph;
        assert_eq!(g.nodes_of("ETN_LoadImageBase64").len(), 2);
        assert_eq!(g.nodes_of("ETN_LoadMaskBase64").len(), 2);
        let use_mask = input(&prepared, "Use", "mask");
        assert_eq!(g.class(use_mask[0].as_str().unwrap()), Some("ETN_LoadMaskBase64"));
        // set to the entire image, it gets the whole canvas
        let whole = Workflow::parse("Fill", &text.replace("automatic", "entire image")).unwrap();
        let mut run = inputs(&[]);
        run.selection = Some(vec![255; 64 * 48]);
        assert_eq!(whole.prepare(&params, &run, &Map::new()).unwrap().area, [0, 0, 64, 48]);
    }

    #[test]
    fn the_starter_workflows_read_and_prepare() {
        let new_picture = Workflow::parse("New", include_str!("../tests/workflows/z-image-new-picture.json")).unwrap();
        let params = new_picture.parameters(&Map::new());
        assert_eq!(params, vec![Param { name: "Prompt".into(), kind: ParamKind::Prompt, default: String::new() }]);
        let prepared = new_picture
            .prepare(
                &params,
                &inputs(&[("Prompt", ParamValue::Text("koi pond, scenery, no humans".into()))]),
                &Map::new(),
            )
            .unwrap();
        assert_eq!(input(&prepared, "KSampler", "seed"), 7);
        assert_eq!(input(&prepared, "CLIPTextEncode", "text"), "koi pond, scenery, no humans");
        let scaled = input(&prepared, "ImageScaleToTotalPixels", "image");
        assert_eq!(prepared.graph.class(scaled[0].as_str().unwrap()), Some("ETN_LoadImageBase64"));
        assert_eq!(prepared.graph.nodes_of("ETN_SendImageWebSocket").len(), 1);

        let paint_over = Workflow::parse("Paint", include_str!("../tests/workflows/z-image-paint-over.json")).unwrap();
        let params = paint_over.parameters(&Map::new());
        assert_eq!(params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Prompt", "Strength"]);
        assert_eq!(params[1].kind, ParamKind::Number { min: Some(0.05), max: Some(1.0), integer: false });
        let mut run = inputs(&[]);
        let mut mask = vec![0u8; 64 * 48];
        mask[20 * 64 + 20] = 255;
        run.selection = Some(mask);
        let prepared = paint_over.prepare(&params, &run, &Map::new()).unwrap();
        assert_eq!(input(&prepared, "KSampler", "denoise"), 0.6);
        // the selection's point and its 32 pixels' padding, kept on the canvas
        assert_eq!(prepared.area, [0, 0, 53, 48]);
        let noise = input(&prepared, "SetLatentNoiseMask", "mask");
        assert_eq!(prepared.graph.class(noise[0].as_str().unwrap()), Some("ETN_LoadMaskBase64"));
    }

    #[test]
    fn the_minimax_starters_read_and_prepare() {
        let new_picture = Workflow::parse("New", include_str!("../tests/workflows/minimax-new-picture.json")).unwrap();
        let params = new_picture.parameters(&Map::new());
        assert_eq!(params.len(), 1);
        let prepared = new_picture
            .prepare(&params, &inputs(&[("Prompt", ParamValue::Text("a ring".into()))]), &Map::new())
            .unwrap();
        assert_eq!(input(&prepared, "MiniMaxH3ImageConditioning", "prompt"), "a ring");
        assert_eq!(input(&prepared, "KSampler", "seed"), 7);

        let edit = Workflow::parse("Edit", include_str!("../tests/workflows/minimax-edit.json")).unwrap();
        let params = edit.parameters(&Map::new());
        let prepared = edit
            .prepare(&params, &inputs(&[("Prompt", ParamValue::Text("make it gold".into()))]), &Map::new())
            .unwrap();
        assert_eq!(input(&prepared, "StringConcatenate", "string_b"), "make it gold");
        let picture = input(&prepared, "MiniMaxH3ImageConditioning", "images.image_1");
        assert_eq!(prepared.graph.class(picture[0].as_str().unwrap()), Some("ETN_LoadImageBase64"));
    }

    #[test]
    fn the_pixel_art_starter_reads_and_prepares() {
        let w = Workflow::parse("Pixel", include_str!("../tests/workflows/rd-flow-pixel-art.json")).unwrap();
        let seed = json!(["INT", {"default": 0, "min": 0, "max": 2_147_483_647}]);
        let background = json!([["transparent", "opaque"], {"default": "transparent"}]);
        let info = json!({
            "RDFlow64Generate": {"input": {"required": {"seed": seed, "background": background}}},
            "RDFlow64SystemPrompt": {"input": {"required": {"seed": seed, "background": background}}},
            "RDFlow64CleanCaption": {"input": {"required": {"background": background}}},
        });
        let info = info.as_object().cloned().unwrap_or_default();
        let params = w.parameters(&info);
        let names: Vec<_> = params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Background", "Palette", "Prompt"]);
        assert_eq!(params[0].kind, ParamKind::Choice(vec!["transparent".into(), "opaque".into()]));
        let mut run = inputs(&[("Prompt", ParamValue::Text("a red potion".into()))]);
        run.seed = 5_000_000_007;
        let prepared = w.prepare(&params, &run, &info).unwrap();
        assert_eq!(input(&prepared, "RDFlow64SystemPrompt", "request"), "a red potion");
        assert_eq!(input(&prepared, "RDFlow64Generate", "background"), "transparent");
        assert_eq!(input(&prepared, "RDFlow64Generate", "palette"), "");
        // the seed wraps into the nodes' range, and stays as it is where no range is known
        assert_eq!(input(&prepared, "RDFlow64Generate", "seed"), 5_000_000_007_u64 % 2_147_483_648);
        assert_eq!(input(&prepared, "ResizeAndPadImage", "target_width"), 64);
        assert_eq!(prepared.outputs.len(), 1);
        assert_eq!(prepared.outputs[0].name, "Pixel art");
    }

    #[test]
    fn the_pixel_edit_starter_reads_and_prepares() {
        let w = Workflow::parse("Pixel edit", include_str!("../tests/workflows/pixel-art-edit.json")).unwrap();
        let params = w.parameters(&Map::new());
        let names: Vec<_> = params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Palette", "Prompt", "Transparent background"]);
        assert_eq!(params[2].kind, ParamKind::Toggle);
        let prepared = w
            .prepare(&params, &inputs(&[("Prompt", ParamValue::Text("give it a sword".into()))]), &Map::new())
            .unwrap();
        assert_eq!(input(&prepared, "StringConcatenate", "string_b"), "give it a sword");
        assert_eq!(input(&prepared, "ComfySwitchNode", "switch"), true);
        // the canvas's size: the snapper's grid, the mask and the sums
        assert_eq!(input(&prepared, "SpriteFusionPixelSnapper", "exact_width"), 64);
        assert_eq!(input(&prepared, "SpriteFusionPixelSnapper", "exact_height"), 48);
        assert_eq!(input(&prepared, "ComfyMathExpression", "values.a"), 64);
        let picture = input(&prepared, "ImageScaleBy", "image");
        assert_eq!(prepared.graph.class(picture[0].as_str().unwrap()), Some("ETN_LoadImageBase64"));
        assert_eq!(prepared.outputs[0].name, "Pixel edit");
    }

    #[test]
    fn the_pixel_animation_starter_reads_and_prepares() {
        let w = Workflow::parse("Animate", include_str!("../tests/workflows/pixel-animation.json")).unwrap();
        let params = w.parameters(&Map::new());
        let names: Vec<_> = params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Frames", "Prompt", "Seconds"]);
        assert_eq!(params[0].kind, ParamKind::Number { min: Some(2.0), max: Some(12.0), integer: true });
        assert_eq!(params[2].kind, ParamKind::Number { min: Some(1.0), max: Some(5.0), integer: false });
        let run = inputs(&[("Prompt", ParamValue::Text("walks to the right".into()))]);
        let prepared = w.prepare(&params, &run, &Map::new()).unwrap();
        assert_eq!(input(&prepared, "StringConcatenate", "string_b"), "walks to the right");
        assert_eq!(input(&prepared, "RandomNoise", "noise_seed"), 7);
        // the canvas's size wherever it's used, and the defaults where the app left them
        assert_eq!(input(&prepared, "SpriteFusionPixelSnapper", "exact_height"), 48);
        assert_eq!(
            input(&prepared, "ImageScaleBy", "image")[0].as_str().map(|n| prepared.graph.class(n)),
            Some(Some("ETN_LoadImageBase64"))
        );
        let maths = prepared.graph.nodes_of("ComfyMathExpression");
        let values: Vec<Value> =
            maths.iter().filter_map(|n| prepared.graph.inputs(n).and_then(|i| i.get("values.a")).cloned()).collect();
        assert!(values.contains(&json!(64)) && values.contains(&json!(48)) && values.contains(&json!(1.6)));
        assert!(values.iter().all(|v| !v.is_array() || v[0].as_str().map(|n| prepared.graph.class(n)) != Some(Some("ETN_Parameter"))));
        assert_eq!(prepared.outputs.len(), 1);
        assert_eq!(prepared.outputs[0].name, "", "the frames are named after the move");
        assert!(prepared.outputs[0].animation);
    }

    #[test]
    fn seeds_fit_a_node_s_range() {
        let spec = json!(["INT", {"min": 10, "max": 19}]);
        assert_eq!(fit_seed(15, Some(&spec)), 15);
        assert_eq!(fit_seed(25, Some(&spec)), 15);
        assert_eq!(fit_seed(26, Some(&spec)), 16);
        assert_eq!(fit_seed(u64::MAX, None), u64::MAX);
        assert_eq!(fit_seed(3_000_000_000, Some(&json!(["INT", {"max": 1.8446744073709552e19}]))), 3_000_000_000);
    }

    #[test]
    fn repeated_names_are_numbered() {
        let text = r#"{
            "1": {"class_type": "ETN_Parameter", "inputs": {"name": "Strength", "type": "number", "default": "0.5"}},
            "2": {"class_type": "ETN_Parameter", "inputs": {"name": "Strength", "type": "number", "default": "0.8"}},
            "3": {"class_type": "Use", "inputs": {"a": ["1", 0], "b": ["2", 0]}},
            "4": {"class_type": "ETN_KritaOutput", "inputs": {"images": ["3", 0]}}
        }"#;
        let w = Workflow::parse("Twice", text).unwrap();
        let params = w.parameters(&Map::new());
        assert_eq!(params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Strength", "Strength (2)"]);
        let prepared = w.prepare(&params, &inputs(&[("Strength (2)", ParamValue::Number(0.25))]), &Map::new()).unwrap();
        assert_eq!(input(&prepared, "Use", "a"), 0.5);
        assert_eq!(input(&prepared, "Use", "b"), 0.25);
    }
}
