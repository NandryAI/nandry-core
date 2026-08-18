use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub use nandry_vm::MAX_U24;
use nandry_vm::{CircuitView, Error as VmError, HEADER_LEN, MAGIC, OP_LATCH, OP_NAND, VERSION};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    #[serde(rename = "INPUT")]
    Input,
    #[serde(rename = "OUTPUT")]
    Output,
    #[serde(rename = "NAND")]
    Nand,
    #[serde(rename = "LATCH")]
    Latch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CircuitGraphNode {
    pub id: String,
    pub kind: NodeKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircuitGraphEndpoint {
    pub node_id: String,
    pub port: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CircuitGraphConnection {
    pub from: CircuitGraphEndpoint,
    pub to: CircuitGraphEndpoint,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CircuitGraph {
    pub nodes: Vec<CircuitGraphNode>,
    pub connections: Vec<CircuitGraphConnection>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetlistGate {
    Nand { a: u32, b: u32 },
    Latch { d: u32 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Netlist {
    pub input_count: u32,
    pub outputs: Vec<u32>,
    pub gates: Vec<NetlistGate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileError {
    EmptyNodeId,
    DuplicateNode(String),
    UnknownNode(String),
    InvalidSourcePort { node: String, port: String },
    InvalidTargetPort { node: String, port: String },
    MultipleDrivers { node: String, port: String },
    MissingDriver { node: String, port: String },
    MissingSignal { node: String, port: String },
    EmptyInputs,
    EmptyOutputs,
    CombinationalCycle,
    CountOverflow,
    SignalSpaceExhausted,
    InvalidOutput { output: u32, signal: u32 },
    InvalidNandReference { gate: u32, signal: u32 },
    InvalidLatchReference { gate: u32, signal: u32 },
    NonCanonicalLatchOrder { gate: u32 },
    BodyTooLarge,
    EncodedNetlistInvalid(VmError),
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyNodeId => formatter.write_str("empty node id"),
            Self::DuplicateNode(node) => write!(formatter, "duplicate node id: {node}"),
            Self::UnknownNode(node) => {
                write!(formatter, "connection references unknown node: {node}")
            }
            Self::InvalidSourcePort { node, port } => {
                write!(formatter, "invalid source port {node}.{port}")
            }
            Self::InvalidTargetPort { node, port } => {
                write!(formatter, "invalid target port {node}.{port}")
            }
            Self::MultipleDrivers { node, port } => {
                write!(formatter, "multiple drivers for {node}.{port}")
            }
            Self::MissingDriver { node, port } => {
                write!(formatter, "missing driver for {node}.{port}")
            }
            Self::MissingSignal { node, port } => {
                write!(formatter, "missing signal for {node}.{port}")
            }
            Self::EmptyInputs => formatter.write_str("circuit requires at least one input"),
            Self::EmptyOutputs => formatter.write_str("circuit requires at least one output"),
            Self::CombinationalCycle => formatter.write_str("combinational NAND cycle"),
            Self::CountOverflow => formatter.write_str("circuit count overflow"),
            Self::SignalSpaceExhausted => formatter.write_str("signal space exhausted"),
            Self::InvalidOutput { output, signal } => {
                write!(
                    formatter,
                    "output {output} references invalid signal {signal}"
                )
            }
            Self::InvalidNandReference { gate, signal } => {
                write!(formatter, "NAND {gate} contains invalid reference {signal}")
            }
            Self::InvalidLatchReference { gate, signal } => {
                write!(
                    formatter,
                    "LATCH {gate} contains invalid reference {signal}"
                )
            }
            Self::NonCanonicalLatchOrder { gate } => {
                write!(formatter, "LATCH {gate} is outside the canonical prefix")
            }
            Self::BodyTooLarge => formatter.write_str("TSOL body is too large"),
            Self::EncodedNetlistInvalid(error) => {
                write!(formatter, "encoded TSOL is invalid: {error}")
            }
        }
    }
}

impl std::error::Error for CompileError {}

pub fn encode_circuit_graph(graph: &CircuitGraph) -> Result<Vec<u8>, CompileError> {
    let mut nodes = BTreeMap::new();
    for node in &graph.nodes {
        if node.id.is_empty() {
            return Err(CompileError::EmptyNodeId);
        }
        if nodes.insert(node.id.clone(), node.kind).is_some() {
            return Err(CompileError::DuplicateNode(node.id.clone()));
        }
    }

    let mut drivers = BTreeMap::<(String, String), String>::new();
    for connection in &graph.connections {
        let source_kind = *nodes
            .get(&connection.from.node_id)
            .ok_or_else(|| CompileError::UnknownNode(connection.from.node_id.clone()))?;
        let target_kind = *nodes
            .get(&connection.to.node_id)
            .ok_or_else(|| CompileError::UnknownNode(connection.to.node_id.clone()))?;
        if output_port(source_kind) != Some(connection.from.port.as_str()) {
            return Err(CompileError::InvalidSourcePort {
                node: connection.from.node_id.clone(),
                port: connection.from.port.clone(),
            });
        }
        if !input_ports(target_kind).contains(&connection.to.port.as_str()) {
            return Err(CompileError::InvalidTargetPort {
                node: connection.to.node_id.clone(),
                port: connection.to.port.clone(),
            });
        }
        let key = (connection.to.node_id.clone(), connection.to.port.clone());
        if drivers
            .insert(key, connection.from.node_id.clone())
            .is_some()
        {
            return Err(CompileError::MultipleDrivers {
                node: connection.to.node_id.clone(),
                port: connection.to.port.clone(),
            });
        }
    }

    let inputs = node_ids(&nodes, NodeKind::Input);
    let outputs = node_ids(&nodes, NodeKind::Output);
    let latches = node_ids(&nodes, NodeKind::Latch);
    if inputs.is_empty() {
        return Err(CompileError::EmptyInputs);
    }
    if outputs.is_empty() {
        return Err(CompileError::EmptyOutputs);
    }
    for (node, kind) in &nodes {
        for port in input_ports(*kind) {
            if !drivers.contains_key(&(node.clone(), (*port).to_owned())) {
                return Err(CompileError::MissingDriver {
                    node: node.clone(),
                    port: (*port).to_owned(),
                });
            }
        }
    }

    let nand = topological_nand_order(&nodes, &drivers)?;
    let input_count = u32::try_from(inputs.len()).map_err(|_| CompileError::CountOverflow)?;
    let gate_count = latches
        .len()
        .checked_add(nand.len())
        .ok_or(CompileError::CountOverflow)?;
    let gate_count_u32 = u32::try_from(gate_count).map_err(|_| CompileError::CountOverflow)?;
    let signal_count = 2u32
        .checked_add(input_count)
        .and_then(|count| count.checked_add(gate_count_u32))
        .ok_or(CompileError::CountOverflow)?;
    if signal_count - 1 > MAX_U24 {
        return Err(CompileError::SignalSpaceExhausted);
    }

    let mut signals = BTreeMap::new();
    for (index, node) in inputs.iter().enumerate() {
        signals.insert(
            node.clone(),
            2 + u32::try_from(index).map_err(|_| CompileError::CountOverflow)?,
        );
    }
    for (index, node) in latches.iter().enumerate() {
        signals.insert(
            node.clone(),
            2 + input_count + u32::try_from(index).map_err(|_| CompileError::CountOverflow)?,
        );
    }
    for (index, node) in nand.iter().enumerate() {
        signals.insert(
            node.clone(),
            2 + input_count
                + u32::try_from(latches.len()).map_err(|_| CompileError::CountOverflow)?
                + u32::try_from(index).map_err(|_| CompileError::CountOverflow)?,
        );
    }

    let output_signals = outputs
        .iter()
        .map(|node| source_signal(node, "in", &drivers, &signals))
        .collect::<Result<Vec<_>, _>>()?;
    let mut gates = Vec::with_capacity(gate_count);
    for node in &latches {
        gates.push(NetlistGate::Latch {
            d: source_signal(node, "d", &drivers, &signals)?,
        });
    }
    for node in &nand {
        gates.push(NetlistGate::Nand {
            a: source_signal(node, "a", &drivers, &signals)?,
            b: source_signal(node, "b", &drivers, &signals)?,
        });
    }
    encode_netlist(&Netlist {
        input_count,
        outputs: output_signals,
        gates,
    })
}

pub fn encode_netlist(netlist: &Netlist) -> Result<Vec<u8>, CompileError> {
    let gate_count = u32::try_from(netlist.gates.len()).map_err(|_| CompileError::CountOverflow)?;
    let output_count =
        u32::try_from(netlist.outputs.len()).map_err(|_| CompileError::CountOverflow)?;
    let signal_count = 2u32
        .checked_add(netlist.input_count)
        .and_then(|count| count.checked_add(gate_count))
        .ok_or(CompileError::CountOverflow)?;
    if signal_count == 0 || signal_count - 1 > MAX_U24 {
        return Err(CompileError::SignalSpaceExhausted);
    }
    for (output, signal) in netlist.outputs.iter().copied().enumerate() {
        if signal >= signal_count {
            return Err(CompileError::InvalidOutput {
                output: u32::try_from(output).map_err(|_| CompileError::CountOverflow)?,
                signal,
            });
        }
    }

    let mut latch_count = 0u32;
    let mut saw_nand = false;
    let mut body_len = 0usize;
    for (index, gate) in netlist.gates.iter().copied().enumerate() {
        let gate_index = u32::try_from(index).map_err(|_| CompileError::CountOverflow)?;
        let current_signal = 2 + netlist.input_count + gate_index;
        match gate {
            NetlistGate::Nand { a, b } => {
                saw_nand = true;
                if a >= current_signal {
                    return Err(CompileError::InvalidNandReference {
                        gate: gate_index,
                        signal: a,
                    });
                }
                if b >= current_signal {
                    return Err(CompileError::InvalidNandReference {
                        gate: gate_index,
                        signal: b,
                    });
                }
                body_len = body_len.checked_add(7).ok_or(CompileError::BodyTooLarge)?;
            }
            NetlistGate::Latch { d } => {
                if saw_nand {
                    return Err(CompileError::NonCanonicalLatchOrder { gate: gate_index });
                }
                if d >= signal_count {
                    return Err(CompileError::InvalidLatchReference {
                        gate: gate_index,
                        signal: d,
                    });
                }
                latch_count = latch_count
                    .checked_add(1)
                    .ok_or(CompileError::CountOverflow)?;
                body_len = body_len.checked_add(4).ok_or(CompileError::BodyTooLarge)?;
            }
        }
    }
    let body_len_u32 = u32::try_from(body_len).map_err(|_| CompileError::BodyTooLarge)?;
    let output_bytes = netlist
        .outputs
        .len()
        .checked_mul(3)
        .ok_or(CompileError::BodyTooLarge)?;
    let capacity = HEADER_LEN
        .checked_add(output_bytes)
        .and_then(|length| length.checked_add(body_len))
        .ok_or(CompileError::BodyTooLarge)?;
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&[VERSION, 0, 0, 0]);
    bytes.extend_from_slice(&netlist.input_count.to_le_bytes());
    bytes.extend_from_slice(&output_count.to_le_bytes());
    bytes.extend_from_slice(&gate_count.to_le_bytes());
    bytes.extend_from_slice(&latch_count.to_le_bytes());
    bytes.extend_from_slice(&body_len_u32.to_le_bytes());
    for signal in &netlist.outputs {
        push_u24(&mut bytes, *signal);
    }
    for gate in &netlist.gates {
        match *gate {
            NetlistGate::Nand { a, b } => {
                bytes.push(OP_NAND);
                push_u24(&mut bytes, a);
                push_u24(&mut bytes, b);
            }
            NetlistGate::Latch { d } => {
                bytes.push(OP_LATCH);
                push_u24(&mut bytes, d);
            }
        }
    }
    CircuitView::parse(&bytes).map_err(CompileError::EncodedNetlistInvalid)?;
    Ok(bytes)
}

fn node_ids(nodes: &BTreeMap<String, NodeKind>, kind: NodeKind) -> Vec<String> {
    nodes
        .iter()
        .filter_map(|(node, node_kind)| (*node_kind == kind).then_some(node.clone()))
        .collect()
}

fn topological_nand_order(
    nodes: &BTreeMap<String, NodeKind>,
    drivers: &BTreeMap<(String, String), String>,
) -> Result<Vec<String>, CompileError> {
    let nand = node_ids(nodes, NodeKind::Nand);
    let mut dependencies = nand
        .iter()
        .map(|node| (node.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    let mut dependents = dependencies.clone();
    for node in &nand {
        for port in ["a", "b"] {
            let Some(source) = drivers.get(&(node.clone(), port.to_owned())) else {
                continue;
            };
            if nodes.get(source) != Some(&NodeKind::Nand) {
                continue;
            }
            dependencies
                .get_mut(node)
                .expect("NAND dependency exists")
                .insert(source.clone());
            dependents
                .get_mut(source)
                .expect("NAND dependent exists")
                .insert(node.clone());
        }
    }
    let mut ready = dependencies
        .iter()
        .filter_map(|(node, remaining)| remaining.is_empty().then_some(node.clone()))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(nand.len());
    while let Some(node) = ready.pop_first() {
        ordered.push(node.clone());
        for dependent in dependents.get(&node).expect("NAND dependents exist") {
            let remaining = dependencies
                .get_mut(dependent)
                .expect("NAND dependency exists");
            remaining.remove(&node);
            if remaining.is_empty() {
                ready.insert(dependent.clone());
            }
        }
    }
    if ordered.len() != nand.len() {
        return Err(CompileError::CombinationalCycle);
    }
    Ok(ordered)
}

fn source_signal(
    node: &str,
    port: &str,
    drivers: &BTreeMap<(String, String), String>,
    signals: &BTreeMap<String, u32>,
) -> Result<u32, CompileError> {
    let source = drivers
        .get(&(node.to_owned(), port.to_owned()))
        .ok_or_else(|| CompileError::MissingDriver {
            node: node.to_owned(),
            port: port.to_owned(),
        })?;
    signals
        .get(source)
        .copied()
        .ok_or_else(|| CompileError::MissingSignal {
            node: node.to_owned(),
            port: port.to_owned(),
        })
}

const fn output_port(kind: NodeKind) -> Option<&'static str> {
    match kind {
        NodeKind::Input | NodeKind::Nand => Some("out"),
        NodeKind::Latch => Some("q"),
        NodeKind::Output => None,
    }
}

const fn input_ports(kind: NodeKind) -> &'static [&'static str] {
    match kind {
        NodeKind::Nand => &["a", "b"],
        NodeKind::Latch => &["d"],
        NodeKind::Output => &["in"],
        NodeKind::Input => &[],
    }
}

fn push_u24(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes()[..3]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(node_id: &str, port: &str) -> CircuitGraphEndpoint {
        CircuitGraphEndpoint {
            node_id: node_id.to_owned(),
            port: port.to_owned(),
        }
    }

    #[test]
    fn graph_compiler_matches_the_tsol_v1_golden_vector() {
        let graph = CircuitGraph {
            nodes: vec![
                CircuitGraphNode {
                    id: "output".into(),
                    kind: NodeKind::Output,
                },
                CircuitGraphNode {
                    id: "nand".into(),
                    kind: NodeKind::Nand,
                },
                CircuitGraphNode {
                    id: "input-b".into(),
                    kind: NodeKind::Input,
                },
                CircuitGraphNode {
                    id: "input-a".into(),
                    kind: NodeKind::Input,
                },
            ],
            connections: vec![
                CircuitGraphConnection {
                    from: endpoint("input-a", "out"),
                    to: endpoint("nand", "a"),
                },
                CircuitGraphConnection {
                    from: endpoint("input-b", "out"),
                    to: endpoint("nand", "b"),
                },
                CircuitGraphConnection {
                    from: endpoint("nand", "out"),
                    to: endpoint("output", "in"),
                },
            ],
        };
        let expected = [
            0x54, 0x53, 0x4f, 0x4c, 1, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 7,
            0, 0, 0, 4, 0, 0, 0, 2, 0, 0, 3, 0, 0,
        ];
        assert_eq!(encode_circuit_graph(&graph).unwrap(), expected);
    }

    #[test]
    fn graph_compiler_rejects_a_combinational_cycle() {
        let graph = CircuitGraph {
            nodes: vec![
                CircuitGraphNode {
                    id: "input".into(),
                    kind: NodeKind::Input,
                },
                CircuitGraphNode {
                    id: "a".into(),
                    kind: NodeKind::Nand,
                },
                CircuitGraphNode {
                    id: "b".into(),
                    kind: NodeKind::Nand,
                },
                CircuitGraphNode {
                    id: "output".into(),
                    kind: NodeKind::Output,
                },
            ],
            connections: vec![
                CircuitGraphConnection {
                    from: endpoint("b", "out"),
                    to: endpoint("a", "a"),
                },
                CircuitGraphConnection {
                    from: endpoint("input", "out"),
                    to: endpoint("a", "b"),
                },
                CircuitGraphConnection {
                    from: endpoint("a", "out"),
                    to: endpoint("b", "a"),
                },
                CircuitGraphConnection {
                    from: endpoint("input", "out"),
                    to: endpoint("b", "b"),
                },
                CircuitGraphConnection {
                    from: endpoint("a", "out"),
                    to: endpoint("output", "in"),
                },
            ],
        };
        assert_eq!(
            encode_circuit_graph(&graph),
            Err(CompileError::CombinationalCycle)
        );
    }

    #[test]
    fn canonical_encoding_is_invariant_under_graph_ordering() {
        let graph = CircuitGraph {
            nodes: vec![
                CircuitGraphNode {
                    id: "output-z".into(),
                    kind: NodeKind::Output,
                },
                CircuitGraphNode {
                    id: "nand-b".into(),
                    kind: NodeKind::Nand,
                },
                CircuitGraphNode {
                    id: "input-b".into(),
                    kind: NodeKind::Input,
                },
                CircuitGraphNode {
                    id: "latch-a".into(),
                    kind: NodeKind::Latch,
                },
                CircuitGraphNode {
                    id: "nand-a".into(),
                    kind: NodeKind::Nand,
                },
                CircuitGraphNode {
                    id: "input-a".into(),
                    kind: NodeKind::Input,
                },
            ],
            connections: vec![
                CircuitGraphConnection {
                    from: endpoint("input-a", "out"),
                    to: endpoint("nand-a", "a"),
                },
                CircuitGraphConnection {
                    from: endpoint("latch-a", "q"),
                    to: endpoint("nand-a", "b"),
                },
                CircuitGraphConnection {
                    from: endpoint("nand-a", "out"),
                    to: endpoint("nand-b", "a"),
                },
                CircuitGraphConnection {
                    from: endpoint("input-b", "out"),
                    to: endpoint("nand-b", "b"),
                },
                CircuitGraphConnection {
                    from: endpoint("nand-b", "out"),
                    to: endpoint("latch-a", "d"),
                },
                CircuitGraphConnection {
                    from: endpoint("latch-a", "q"),
                    to: endpoint("output-z", "in"),
                },
            ],
        };
        let canonical = encode_circuit_graph(&graph).unwrap();

        for node_rotation in 0..graph.nodes.len() {
            for connection_rotation in 0..graph.connections.len() {
                let mut reordered = graph.clone();
                reordered.nodes.rotate_left(node_rotation);
                reordered.connections.rotate_left(connection_rotation);
                if (node_rotation + connection_rotation) % 2 == 1 {
                    reordered.nodes.reverse();
                    reordered.connections.reverse();
                }
                assert_eq!(encode_circuit_graph(&reordered).unwrap(), canonical);
            }
        }
    }
}
