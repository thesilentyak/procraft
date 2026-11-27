//! Workflows from ComfyUI in the Generate popover (`docs/generation/10-workflows-design.md`): the
//! live one ComfyUI's tab sends, and the kept ones, files in the styles folder's Workflows folder
//! that the apps read and hand over as they do the styles. A workflow card shows its parameters
//! instead of a style's settings, and a run of it lands in the results tray like any other.

use std::collections::BTreeMap;

use parking_lot::Mutex;
use procraft_diffusion::{
    Connection, Param, ParamKind, ParamValue, ServerState, Workflow, WorkflowInputs, WorkflowRequest,
};
use procraft_engine::geom::IRect;

use crate::CanvasSession;
use crate::generation::{Generation, GenerationError, layer_name, random_seed};

/// The live workflow's card id.
pub const LIVE_ID: &str = "live";
/// The live workflow's card name.
const LIVE_NAME: &str = "ComfyUI (live)";
/// A kept workflow's card id: this and its file's name.
const FILE_PREFIX: &str = "file:";
/// The most pictures a workflow run makes, as for styles.
const MOST_RUNS: u32 = 4;

/// The workflows the apps have handed over.
#[derive(Default)]
pub(crate) struct Workflows {
    /// The Workflows folder's files, as the app last read them.
    files: Mutex<Vec<WorkflowFileText>>,
    /// The live workflow as the app kept it from an earlier start (its text), for when the server
    /// hasn't sent one yet.
    stored_live: Mutex<Option<String>>,
}

/// A workflow file: its name (`Koi pond.json`) and text.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct WorkflowFileText {
    pub name: String,
    pub text: String,
}

/// A workflow as the popover's cards show it.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct WorkflowCard {
    /// `live`, or `file:` and the file's name.
    pub id: String,
    pub name: String,
    pub live: bool,
    /// Why it can't run (nothing received yet, a file that isn't a workflow); none when it can.
    pub reason: Option<String>,
}

/// Which control a parameter gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ParameterKind {
    /// The prompt field, with suggestions.
    Prompt,
    /// A plain text field, two lines.
    NegativePrompt,
    /// A one-line text field.
    Text,
    /// A slider when `min` and `max` are set, else a number field.
    Number,
    /// The same, in whole steps.
    Integer,
    Toggle,
    /// A list of `choices`.
    Choice,
    /// A layer picker; the layer goes in as a picture.
    ImageLayer,
    /// A layer picker; the layer's alpha goes in as a mask.
    MaskLayer,
}

/// One row of the popover's Parameters part.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ParameterInfo {
    /// Its label and key: values are given and remembered by it.
    pub name: String,
    pub kind: ParameterKind,
    /// What the row starts at (`"true"`, `"0.75"`, a choice, a prompt).
    pub default: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub choices: Vec<String>,
    /// Shown without opening More: the prompts.
    pub in_view: bool,
}

/// A row's value for a run.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum ParameterValue {
    Text {
        name: String,
        value: String,
    },
    Number {
        name: String,
        value: f64,
    },
    Toggle {
        name: String,
        value: bool,
    },
    /// A layer picker's layer.
    Layer {
        name: String,
        layer: u64,
    },
}

/// What a workflow run is asked for: the card, the rows' values (one not given keeps its
/// default; a layer picker not given takes the layer of its name, else the current layer), the
/// seed (none: a random one) and how many pictures.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct WorkflowRun {
    pub workflow: String,
    pub values: Vec<ParameterValue>,
    pub seed: Option<u64>,
    pub count: u32,
}

impl From<&Param> for ParameterInfo {
    fn from(p: &Param) -> ParameterInfo {
        let (kind, min, max, choices) = match &p.kind {
            ParamKind::Prompt => (ParameterKind::Prompt, None, None, Vec::new()),
            ParamKind::NegativePrompt => (ParameterKind::NegativePrompt, None, None, Vec::new()),
            ParamKind::Text => (ParameterKind::Text, None, None, Vec::new()),
            ParamKind::Number { min, max, integer } => {
                (if *integer { ParameterKind::Integer } else { ParameterKind::Number }, *min, *max, Vec::new())
            }
            ParamKind::Toggle => (ParameterKind::Toggle, None, None, Vec::new()),
            ParamKind::Choice(list) => (ParameterKind::Choice, None, None, list.clone()),
            ParamKind::ImageLayer => (ParameterKind::ImageLayer, None, None, Vec::new()),
            ParamKind::MaskLayer => (ParameterKind::MaskLayer, None, None, Vec::new()),
        };
        let default = match (&p.kind, p.default.is_empty()) {
            (ParamKind::Choice(list), true) => list.first().cloned().unwrap_or_default(),
            _ => p.default.clone(),
        };
        ParameterInfo { name: p.name.clone(), kind, default, min, max, choices, in_view: kind == ParameterKind::Prompt }
    }
}

/// The card name of workflow file `name`: without its `.json`.
fn card_name(name: &str) -> String {
    name.strip_suffix(".json").unwrap_or(name).to_owned()
}

#[uniffi::export]
impl Generation {
    /// The Workflows folder's files, read by the app (each `.json` there): used from now on.
    pub fn set_workflow_files(&self, files: Vec<WorkflowFileText>) {
        *self.workflows.files.lock() = files;
    }

    /// The live workflow the app kept from an earlier start (see
    /// [`live_workflow_text`](Self::live_workflow_text)), until the server sends one.
    pub fn set_live_workflow(&self, text: Option<String>) {
        *self.workflows.stored_live.lock() = text;
    }

    /// The revision of the live workflow the server sent, one more for each change; 0 before any.
    /// The app keeps [`live_workflow_text`](Self::live_workflow_text) when it goes up.
    pub fn live_workflow_revision(&self) -> u64 {
        self.worker.server().live_workflow.map_or(0, |live| live.revision)
    }

    /// The live workflow's text, to keep for the next start; none before any.
    pub fn live_workflow_text(&self) -> Option<String> {
        match self.worker.server().live_workflow {
            Some(live) => Some(live.graph.to_string()),
            None => self.workflows.stored_live.lock().clone(),
        }
    }

    /// The workflow cards, in their order: the live one (once a server is set), then the kept ones
    /// by name. Each says why it can't run, if it can't.
    pub fn workflow_cards(&self) -> Vec<WorkflowCard> {
        let server = self.worker.server();
        let mut cards = Vec::new();
        if server.address.is_some() {
            let reason = match self.live_workflow(&server) {
                Some(Ok(_)) => None,
                Some(Err(e)) => Some(e.to_string()),
                None if server.connection == Connection::Connected && !server.workflow_link => Some(
                    "The server's comfyui-tooling-nodes add-on can't send workflows: update it in ComfyUI's Manager"
                        .to_owned(),
                ),
                None => Some("Open a workflow with a Krita Output node in ComfyUI".to_owned()),
            };
            cards.push(WorkflowCard { id: LIVE_ID.to_owned(), name: LIVE_NAME.to_owned(), live: true, reason });
        }
        let mut files = self.workflows.files.lock().clone();
        files.sort_by_key(|f| f.name.to_lowercase());
        for file in files {
            let reason = Workflow::parse(&card_name(&file.name), &file.text).err().map(|e| e.to_string());
            cards.push(WorkflowCard {
                id: format!("{FILE_PREFIX}{}", file.name),
                name: card_name(&file.name),
                live: false,
                reason,
            });
        }
        cards
    }

    /// Workflow `id`'s rows, sorted by name; empty when it can't be read.
    pub fn workflow_parameters(&self, id: String) -> Vec<ParameterInfo> {
        let server = self.worker.server();
        match self.workflow_by_id(&server, &id) {
            Ok(workflow) => workflow.parameters(&server.node_info).iter().map(ParameterInfo::from).collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The live workflow as a file named `name` (`.json` added), for the app to write into the
    /// Workflows folder: Keep.
    pub fn keep_workflow(&self, name: String) -> Result<WorkflowFileText, GenerationError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(GenerationError::NotReady { reason: "the workflow needs a name".to_owned() });
        }
        let server = self.worker.server();
        let workflow = match self.live_workflow(&server) {
            Some(Ok(workflow)) => workflow,
            Some(Err(e)) => return Err(GenerationError::NotReady { reason: e.to_string() }),
            None => {
                return Err(GenerationError::NotReady { reason: "no workflow has come from ComfyUI yet".to_owned() });
            }
        };
        let text = serde_json::to_string_pretty(&workflow.to_json())
            .map_err(|e| GenerationError::NotReady { reason: e.to_string() })?;
        let file = if name.to_lowercase().ends_with(".json") { name.to_owned() } else { format!("{name}.json") };
        Ok(WorkflowFileText { name: file, text })
    }

    /// Checks a file brought in with Import: none if it's a workflow the app can run, else why
    /// not, in words for the person ("in ComfyUI use Workflow › Export (API)").
    pub fn check_workflow_file(&self, text: String) -> Option<String> {
        Workflow::parse("Imported", &text).err().map(|e| e.to_string())
    }

    /// Starts a run of workflow `run.workflow` on `session`'s canvas, `run.count` pictures, seeds
    /// one apart. Its pictures arrive in the results tray as a style's do. Returns the run's id.
    pub fn generate_workflow(&self, session: &CanvasSession, run: WorkflowRun) -> Result<u64, GenerationError> {
        let server = self.worker.server();
        if server.connection != Connection::Connected {
            return Err(GenerationError::NotReady { reason: "not connected to a server".to_owned() });
        }
        let workflow = self
            .workflow_by_id(&server, &run.workflow)
            .map_err(|e| GenerationError::NotReady { reason: e.to_string() })?;
        let params = workflow.parameters(&server.node_info);
        let canvas = session.workflow_canvas()?;
        let mut values = BTreeMap::new();
        let mut layers = BTreeMap::new();
        for value in &run.values {
            match value {
                ParameterValue::Text { name, value } => {
                    values.insert(name.clone(), ParamValue::Text(value.clone()));
                }
                ParameterValue::Number { name, value } => {
                    values.insert(name.clone(), ParamValue::Number(*value));
                }
                ParameterValue::Toggle { name, value } => {
                    values.insert(name.clone(), ParamValue::Toggle(*value));
                }
                ParameterValue::Layer { name, layer } => {
                    if let Some(picture) = session.layer_picture(*layer)? {
                        layers.insert(name.clone(), picture);
                    }
                }
            }
        }
        // a layer picker left alone: the layer named like it, else the current layer
        for param in params.iter().filter(|p| matches!(p.kind, ParamKind::ImageLayer | ParamKind::MaskLayer)) {
            if layers.contains_key(&param.name) {
                continue;
            }
            if let Some(id) = default_layer(session, &param.name)
                && let Some(picture) = session.layer_picture(id)?
            {
                layers.insert(param.name.clone(), picture);
            }
        }
        let prompt = params
            .iter()
            .find(|p| p.kind == ParamKind::Prompt)
            .map(|p| match values.get(&p.name) {
                Some(ParamValue::Text(text)) => text.clone(),
                _ => p.default.clone(),
            })
            .unwrap_or_default();
        let size = (canvas.picture.width, canvas.picture.height);
        let selection = canvas.selection.filter(|mask| mask.iter().any(|&v| v > 0));
        let [x, y, w, h] = workflow.area(selection.as_deref(), size);
        let area = IRect::new(
            i32::try_from(x).unwrap_or(0),
            i32::try_from(y).unwrap_or(0),
            i32::try_from(x.saturating_add(w)).unwrap_or(i32::MAX),
            i32::try_from(y.saturating_add(h)).unwrap_or(i32::MAX),
        );
        let mask = selection.as_deref().map(|m| crop(m, size.0, [x, y, w, h]));
        let name = if prompt.trim().is_empty() { workflow.name.clone() } else { layer_name(&prompt) };
        if !prompt.trim().is_empty() {
            crate::generation_tags::remember(&prompt);
        }
        let (colours, reference) = (canvas.colours, canvas.reference);
        let request = WorkflowRequest {
            workflow,
            params,
            inputs: WorkflowInputs {
                canvas: canvas.picture,
                selection,
                seed: run.seed.unwrap_or_else(random_seed),
                values,
                layers,
            },
            count: run.count.clamp(1, MOST_RUNS),
        };
        let id =
            self.start_run(session, area, mask, name, canvas.from_srgb, move |worker| worker.submit_workflow(request))?;
        self.keep_run_reference(session, colours, reference);
        Ok(id)
    }
}

impl Generation {
    /// The live workflow: the one the server sent, else the one the app kept; none before any,
    /// an error if it can't be read.
    fn live_workflow(&self, server: &ServerState) -> Option<Result<Workflow, procraft_diffusion::Error>> {
        if let Some(live) = &server.live_workflow {
            return Some(Workflow::from_value(LIVE_NAME, &live.graph));
        }
        self.workflows.stored_live.lock().as_deref().map(|text| Workflow::parse(LIVE_NAME, text))
    }

    /// Workflow card `id`'s workflow.
    fn workflow_by_id(&self, server: &ServerState, id: &str) -> Result<Workflow, procraft_diffusion::Error> {
        if id == LIVE_ID {
            return self.live_workflow(server).unwrap_or_else(|| {
                Err(procraft_diffusion::Error::Request {
                    reason: "no workflow has come from ComfyUI yet: open one with a Krita Output node there".to_owned(),
                })
            });
        }
        let name = id.strip_prefix(FILE_PREFIX).unwrap_or(id);
        let files = self.workflows.files.lock();
        let file = files
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| procraft_diffusion::Error::Request { reason: format!("there's no workflow {name}") })?;
        Workflow::parse(&card_name(&file.name), &file.text)
    }
}

/// The layer a picker named `name` takes when left alone: the top layer of that name (any case),
/// else the current layer.
fn default_layer(session: &CanvasSession, name: &str) -> Option<u64> {
    let layers = session.layers();
    let wanted = name.trim().to_lowercase();
    layers
        .iter()
        .find(|l| l.name.trim().to_lowercase() == wanted)
        .or_else(|| layers.iter().find(|l| l.current))
        .map(|l| l.id)
}

/// The part of a canvas-wide mask in `[x, y, w, h]`.
fn crop(mask: &[u8], width: u32, [x, y, w, h]: [u32; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity((w as usize).saturating_mul(h as usize));
    for row in y..y.saturating_add(h) {
        let start = (row as usize).saturating_mul(width as usize).saturating_add(x as usize);
        out.extend_from_slice(mask.get(start..start.saturating_add(w as usize)).unwrap_or_default());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::GenerationSettings;

    const TEXT_TO_IMAGE: &str = r#"{
        "1": {"class_type": "ETN_KritaCanvas", "inputs": {}},
        "2": {"class_type": "ETN_Parameter", "inputs": {"name": "Prompt", "type": "prompt (positive)", "default": ""}},
        "3": {"class_type": "ETN_Parameter", "inputs": {"name": "Steps", "type": "number (integer)", "default": "8", "min": 1, "max": 30}},
        "4": {"class_type": "Make", "inputs": {"text": ["2", 0], "steps": ["3", 0], "w": ["1", 1]}},
        "5": {"class_type": "ETN_KritaOutput", "inputs": {"images": ["4", 0]}}
    }"#;

    fn generation() -> std::sync::Arc<Generation> {
        Generation::new(GenerationSettings::default())
    }

    #[test]
    fn kept_files_are_cards_by_name_with_their_problems() {
        let g = generation();
        g.set_workflow_files(vec![
            WorkflowFileText { name: "zebra.json".into(), text: TEXT_TO_IMAGE.into() },
            WorkflowFileText { name: "Apple.json".into(), text: r#"{"nodes": [], "links": []}"#.into() },
        ]);
        let cards = g.workflow_cards();
        // no server set: no live card
        assert_eq!(cards.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Apple", "zebra"]);
        assert!(cards[0].reason.as_deref().unwrap().contains("Export (API)"));
        assert_eq!(cards[1].reason, None);
        assert_eq!(cards[1].id, "file:zebra.json");
    }

    #[test]
    fn parameters_show_only_the_prompt_in_view() {
        let g = generation();
        g.set_workflow_files(vec![WorkflowFileText { name: "t.json".into(), text: TEXT_TO_IMAGE.into() }]);
        let params = g.workflow_parameters("file:t.json".into());
        assert_eq!(params.len(), 2);
        assert_eq!(
            (params[0].name.as_str(), params[0].kind, params[0].in_view),
            ("Prompt", ParameterKind::Prompt, true)
        );
        assert_eq!(params[1].kind, ParameterKind::Integer);
        assert_eq!((params[1].min, params[1].max, params[1].in_view), (Some(1.0), Some(30.0), false));
        assert!(g.workflow_parameters("file:missing.json".into()).is_empty());
    }

    #[test]
    fn the_stored_live_workflow_stands_in_until_the_server_sends_one() {
        let g = generation();
        assert_eq!(g.live_workflow_revision(), 0);
        assert!(g.keep_workflow("Koi".into()).is_err());
        g.set_live_workflow(Some(TEXT_TO_IMAGE.into()));
        assert_eq!(g.live_workflow_text().as_deref(), Some(TEXT_TO_IMAGE));
        let kept = g.keep_workflow("Koi".into()).unwrap();
        assert_eq!(kept.name, "Koi.json");
        assert!(Workflow::parse("Koi", &kept.text).is_ok());
        assert!(g.keep_workflow("  ".into()).is_err());
        assert_eq!(g.check_workflow_file(TEXT_TO_IMAGE.into()), None);
        assert!(g.check_workflow_file("{}".into()).is_some());
    }

    #[test]
    fn a_selection_is_cropped_to_the_area() {
        let mask: Vec<u8> = (0..16).collect();
        assert_eq!(crop(&mask, 4, [1, 1, 2, 2]), vec![5, 6, 9, 10]);
    }
}
