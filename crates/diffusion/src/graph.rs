//! A ComfyUI graph in its API format, built node by node: `{"1": {"class_type": ..., "inputs":
//! {...}}, ...}`. An input is a value or another node's output, written `[node id, output index]`.

use serde_json::{Map, Value, json};

/// A graph being built; [`Graph::to_json`] gives what `POST /prompt` takes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    nodes: Vec<(String, Map<String, Value>)>,
}

/// A node added to a graph (its id is its place, from 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node(usize);

/// One of a node's outputs, to feed into another node's input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Out {
    node: Node,
    slot: u32,
}

impl Node {
    /// Its output `slot` (0 for a node with one).
    pub fn out(self, slot: u32) -> Out {
        Out { node: self, slot }
    }

    /// Its id in the graph.
    pub fn id(self) -> String {
        (self.0 + 1).to_string()
    }
}

impl From<Node> for Out {
    /// A node's first output.
    fn from(node: Node) -> Out {
        node.out(0)
    }
}

impl From<Out> for Value {
    fn from(out: Out) -> Value {
        json!([out.node.id(), out.slot])
    }
}

impl Graph {
    pub fn new() -> Graph {
        Graph::default()
    }

    /// Adds a node of `class` with `inputs` (values, or other nodes' [`Out`]s).
    pub fn add<'a>(&mut self, class: &str, inputs: impl IntoIterator<Item = (&'a str, Value)>) -> Node {
        let inputs = inputs.into_iter().map(|(name, value)| (name.to_owned(), value)).collect();
        self.nodes.push((class.to_owned(), inputs));
        Node(self.nodes.len() - 1)
    }

    /// Sets input `name` of the node with id `node` to `value` (nothing if there's no such node).
    pub fn set(&mut self, node: &str, name: &str, value: Value) {
        let index = node.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
        if let Some((_, inputs)) = index.and_then(|i| self.nodes.get_mut(i)) {
            inputs.insert(name.to_owned(), value);
        }
    }

    /// How many nodes it has.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The ids of the nodes of `class`.
    pub fn nodes_of(&self, class: &str) -> Vec<String> {
        self.nodes.iter().enumerate().filter(|(_, (c, _))| c == class).map(|(i, _)| Node(i).id()).collect()
    }

    /// A graph made elsewhere (a ComfyUI workflow in its API format), its nodes numbered again
    /// from 1 in the order of their ids, numbers first, and every link to them renumbered to
    /// match, so nodes added later never take an id that's in use. An error names the first node
    /// that isn't `{"class_type": ..., "inputs": {...}}`.
    pub(crate) fn from_api(nodes: &Map<String, Value>) -> Result<Graph, String> {
        let mut ids: Vec<&String> = nodes.keys().collect();
        ids.sort_by(|a, b| match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            (Ok(_), Err(_)) => std::cmp::Ordering::Less,
            (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
            (Err(_), Err(_)) => a.cmp(b),
        });
        let renumbered: std::collections::HashMap<&str, String> =
            ids.iter().enumerate().map(|(i, id)| (id.as_str(), Node(i).id())).collect();
        let mut graph = Graph::new();
        for id in ids {
            let node = nodes.get(id).and_then(Value::as_object);
            let class = node.and_then(|n| n.get("class_type")).and_then(Value::as_str);
            let inputs = node.and_then(|n| n.get("inputs")).and_then(Value::as_object);
            let (Some(class), Some(inputs)) = (class, inputs) else {
                return Err(format!("node {id} isn't a ComfyUI node"));
            };
            let inputs = inputs
                .iter()
                .map(|(name, value)| {
                    let value = match link(value) {
                        Some((to, slot)) => match renumbered.get(to) {
                            Some(new) => json!([new, slot]),
                            None => value.clone(),
                        },
                        None => value.clone(),
                    };
                    (name.clone(), value)
                })
                .collect();
            graph.nodes.push((class.to_owned(), inputs));
        }
        Ok(graph)
    }

    /// The class of the node with id `node`.
    pub(crate) fn class(&self, node: &str) -> Option<&str> {
        self.node(node).map(|(class, _)| class.as_str())
    }

    /// The inputs of the node with id `node`.
    pub(crate) fn inputs(&self, node: &str) -> Option<&Map<String, Value>> {
        self.node(node).map(|(_, inputs)| inputs)
    }

    /// Makes the node with id `node` one of `class` with `inputs`, keeping its id, so links to its
    /// outputs now take the new node's (nothing if there's no such node).
    pub(crate) fn replace<'a>(&mut self, node: &str, class: &str, inputs: impl IntoIterator<Item = (&'a str, Value)>) {
        let index = node.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
        if let Some(entry) = index.and_then(|i| self.nodes.get_mut(i)) {
            *entry = (class.to_owned(), inputs.into_iter().map(|(name, value)| (name.to_owned(), value)).collect());
        }
    }

    /// The nodes and inputs fed by output `slot` of the node with id `node`, in node order.
    pub(crate) fn consumers(&self, node: &str, slot: u64) -> Vec<(String, String)> {
        let mut found = Vec::new();
        for (i, (_, inputs)) in self.nodes.iter().enumerate() {
            for (name, value) in inputs {
                if link(value) == Some((node, slot)) {
                    found.push((Node(i).id(), name.clone()));
                }
            }
        }
        found
    }

    /// Puts `value` in place of every link to output `slot` of the node with id `node`.
    pub(crate) fn relink(&mut self, node: &str, slot: u64, value: &Value) {
        for (_, inputs) in &mut self.nodes {
            for input in inputs.values_mut() {
                if link(input) == Some((node, slot)) {
                    *input = value.clone();
                }
            }
        }
    }

    fn node(&self, node: &str) -> Option<&(String, Map<String, Value>)> {
        node.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).and_then(|i| self.nodes.get(i))
    }

    /// The graph as `POST /prompt`'s `prompt`.
    pub fn to_json(&self) -> Value {
        let nodes = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, (class, inputs))| (Node(i).id(), json!({ "class_type": class, "inputs": inputs })))
            .collect::<Map<_, _>>();
        Value::Object(nodes)
    }
}

/// The node id and output slot `value` links to, if it's a link (`[id, slot]`).
fn link(value: &Value) -> Option<(&str, u64)> {
    match value.as_array().map(Vec::as_slice) {
        Some([Value::String(id), Value::Number(slot)]) => slot.as_u64().map(|slot| (id.as_str(), slot)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_graph_from_elsewhere_is_renumbered_with_its_links() {
        let api = json!({
            "10": { "class_type": "Use", "inputs": { "model": ["4", 0], "steps": 8, "pair": [3, 4] } },
            "4": { "class_type": "Loader", "inputs": { "name": "m.safetensors" } },
            "25:7": { "class_type": "Inner", "inputs": { "model": ["10", 1] } },
        });
        let mut g = Graph::from_api(api.as_object().unwrap()).unwrap();
        assert_eq!(
            g.to_json(),
            json!({
                "1": { "class_type": "Loader", "inputs": { "name": "m.safetensors" } },
                "2": { "class_type": "Use", "inputs": { "model": ["1", 0], "steps": 8, "pair": [3, 4] } },
                "3": { "class_type": "Inner", "inputs": { "model": ["2", 1] } },
            })
        );
        assert_eq!(g.consumers("1", 0), vec![("2".to_owned(), "model".to_owned())]);
        g.relink("2", 1, &json!(42));
        assert_eq!(g.inputs("3").unwrap()["model"], 42);
        g.replace("1", "Other", [("x", json!(1))]);
        assert_eq!(g.class("1"), Some("Other"));
        let added = g.add("New", []);
        assert_eq!(added.id(), "4");
        assert!(Graph::from_api(json!({ "1": { "inputs": {} } }).as_object().unwrap()).is_err());
    }

    #[test]
    fn nodes_are_numbered_and_wired() {
        let mut g = Graph::new();
        let a = g.add("Loader", [("name", json!("model.safetensors"))]);
        let b = g.add("Use", [("model", a.out(0).into()), ("steps", json!(8))]);
        assert_eq!(b.id(), "2");
        assert_eq!(
            g.to_json(),
            json!({
                "1": { "class_type": "Loader", "inputs": { "name": "model.safetensors" } },
                "2": { "class_type": "Use", "inputs": { "model": ["1", 0], "steps": 8 } },
            })
        );
        assert_eq!(g.nodes_of("Use"), vec!["2"]);
        g.set("2", "steps", json!(4));
        g.set("9", "steps", json!(4));
        assert_eq!(g.to_json()["2"]["inputs"]["steps"], 4);
    }
}
