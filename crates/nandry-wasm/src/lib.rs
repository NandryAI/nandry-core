use nandry_compiler::{encode_circuit_graph, CircuitGraph};
use nandry_vm::{CircuitView, Gate};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CircuitMetadata {
    header: HeaderMetadata,
    outputs: Vec<u32>,
    gates: Vec<GateMetadata>,
    signal_count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeaderMetadata {
    input_count: u32,
    output_count: u32,
    gate_count: u32,
    latch_count: u32,
    body_length: u32,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum GateMetadata {
    Nand { a: u32, b: u32 },
    Latch { d: u32 },
}

#[wasm_bindgen(js_name = encodeCircuitGraphJson)]
pub fn encode_circuit_graph_json(graph_json: &str) -> Result<Vec<u8>, JsValue> {
    let graph = serde_json::from_str::<CircuitGraph>(graph_json).map_err(js_error)?;
    encode_circuit_graph(&graph).map_err(js_error)
}

#[wasm_bindgen(js_name = parseCircuitJson)]
pub fn parse_circuit_json(bytes: &[u8]) -> Result<String, JsValue> {
    let circuit = CircuitView::parse(bytes).map_err(js_error)?;
    let header = circuit.header();
    let outputs = (0..header.output_count)
        .map(|index| circuit.output_signal(index))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| JsValue::from_str("validated TSOL output table is incomplete"))?;
    let gates = circuit
        .gates()
        .map(|gate| match gate {
            Gate::Nand { a, b } => GateMetadata::Nand { a, b },
            Gate::Latch { d } => GateMetadata::Latch { d },
        })
        .collect();
    serde_json::to_string(&CircuitMetadata {
        header: HeaderMetadata {
            input_count: header.input_count,
            output_count: header.output_count,
            gate_count: header.gate_count,
            latch_count: header.latch_count,
            body_length: header.body_len,
        },
        outputs,
        gates,
        signal_count: header.signal_count(),
    })
    .map_err(js_error)
}

#[wasm_bindgen(js_name = stepCircuitBytes)]
pub fn step_circuit_bytes(bytes: &[u8], input: &[u8], state: &[u8]) -> Result<Vec<u8>, JsValue> {
    let circuit = CircuitView::parse(bytes).map_err(js_error)?;
    let mut scratch = vec![0; circuit.required_scratch_bytes()];
    let mut next_state = vec![0; circuit.required_state_bytes()];
    let mut output = vec![0; circuit.required_output_bytes()];
    circuit
        .step(input, state, &mut scratch, &mut next_state, &mut output)
        .map_err(js_error)?;
    output.extend_from_slice(&next_state);
    Ok(output)
}

fn js_error(error: impl core::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct GoldenFile {
        version: u8,
        vectors: Vec<GoldenVector>,
    }

    #[derive(Deserialize)]
    struct GoldenVector {
        name: String,
        bytecode_hex: String,
        steps: Vec<GoldenStep>,
    }

    #[derive(Deserialize)]
    struct GoldenStep {
        input_hex: String,
        state_hex: String,
        output_hex: String,
        next_state_hex: String,
    }

    #[test]
    fn wasm_api_matches_shared_execution_vectors() {
        let golden: GoldenFile =
            serde_json::from_str(include_str!("../../../golden/vm-v1.json")).unwrap();
        assert_eq!(golden.version, nandry_vm::VERSION);

        for vector in golden.vectors {
            let bytecode = decode_hex(&vector.bytecode_hex);
            for step in vector.steps {
                let mut expected = decode_hex(&step.output_hex);
                expected.extend_from_slice(&decode_hex(&step.next_state_hex));
                assert_eq!(
                    step_circuit_bytes(
                        &bytecode,
                        &decode_hex(&step.input_hex),
                        &decode_hex(&step.state_hex),
                    )
                    .unwrap(),
                    expected,
                    "{}",
                    vector.name
                );
            }
        }
    }

    fn decode_hex(value: &str) -> Vec<u8> {
        assert_eq!(value.len() % 2, 0);
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let text = core::str::from_utf8(pair).unwrap();
                u8::from_str_radix(text, 16).unwrap()
            })
            .collect()
    }
}
