#![no_std]

#[cfg(feature = "std")]
extern crate std;

use core::fmt;

pub const MAGIC: [u8; 4] = *b"TSOL";
pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 28;
pub const OP_NAND: u8 = 0;
pub const OP_LATCH: u8 = 1;
pub const MAX_U24: u32 = 0x00ff_ffff;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header {
    pub input_count: u32,
    pub output_count: u32,
    pub gate_count: u32,
    pub latch_count: u32,
    pub body_len: u32,
}

impl Header {
    pub fn signal_count(self) -> u32 {
        2 + self.input_count + self.gate_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Gate {
    Nand { a: u32, b: u32 },
    Latch { d: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    TruncatedHeader,
    BadMagic,
    UnsupportedVersion(u8),
    UnsupportedFlags(u8),
    NonZeroReserved,
    SizeOverflow,
    SignalSpaceExhausted,
    LengthMismatch,
    TruncatedOutputTable,
    InvalidOutput {
        output: u32,
        signal: u32,
    },
    UnknownOpcode {
        gate: u32,
        opcode: u8,
    },
    TruncatedGate {
        gate: u32,
    },
    InvalidNandReference {
        gate: u32,
        signal: u32,
    },
    InvalidLatchReference {
        gate: u32,
        signal: u32,
    },
    NonCanonicalLatchOrder {
        gate: u32,
    },
    LatchCountMismatch {
        declared: u32,
        actual: u32,
    },
    InputLength {
        expected: usize,
        actual: usize,
    },
    StateLength {
        expected: usize,
        actual: usize,
    },
    OutputLength {
        expected: usize,
        actual: usize,
    },
    ScratchLength {
        expected_at_least: usize,
        actual: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

#[derive(Clone, Copy)]
pub struct CircuitView<'a> {
    bytes: &'a [u8],
    header: Header,
    outputs_offset: usize,
    body_offset: usize,
}

impl<'a> CircuitView<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let view = Self::parse_layout(bytes)?;
        let signal_count = view.header.signal_count();
        view.validate_outputs(signal_count)?;
        view.validate_gates(signal_count)?;
        Ok(view)
    }

    fn parse_layout(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.len() < HEADER_LEN {
            return Err(Error::TruncatedHeader);
        }
        if bytes[0..4] != MAGIC {
            return Err(Error::BadMagic);
        }
        if bytes[4] != VERSION {
            return Err(Error::UnsupportedVersion(bytes[4]));
        }
        if bytes[5] != 0 {
            return Err(Error::UnsupportedFlags(bytes[5]));
        }
        if bytes[6] != 0 || bytes[7] != 0 {
            return Err(Error::NonZeroReserved);
        }

        let header = Header {
            input_count: read_u32(bytes, 8)?,
            output_count: read_u32(bytes, 12)?,
            gate_count: read_u32(bytes, 16)?,
            latch_count: read_u32(bytes, 20)?,
            body_len: read_u32(bytes, 24)?,
        };
        let signal_count = 2u32
            .checked_add(header.input_count)
            .and_then(|value| value.checked_add(header.gate_count))
            .ok_or(Error::SizeOverflow)?;
        if signal_count == 0 || signal_count - 1 > MAX_U24 {
            return Err(Error::SignalSpaceExhausted);
        }

        let output_bytes = usize::try_from(header.output_count)
            .ok()
            .and_then(|count| count.checked_mul(3))
            .ok_or(Error::SizeOverflow)?;
        let body_offset = HEADER_LEN
            .checked_add(output_bytes)
            .ok_or(Error::SizeOverflow)?;
        if bytes.len() < body_offset {
            return Err(Error::TruncatedOutputTable);
        }
        let expected_len = body_offset
            .checked_add(usize::try_from(header.body_len).map_err(|_| Error::SizeOverflow)?)
            .ok_or(Error::SizeOverflow)?;
        if bytes.len() != expected_len {
            return Err(Error::LengthMismatch);
        }

        Ok(Self {
            bytes,
            header,
            outputs_offset: HEADER_LEN,
            body_offset,
        })
    }

    pub fn header(self) -> Header {
        self.header
    }

    pub fn encoded(self) -> &'a [u8] {
        self.bytes
    }

    pub fn required_input_bytes(self) -> usize {
        bit_bytes(self.header.input_count)
    }

    pub fn required_output_bytes(self) -> usize {
        bit_bytes(self.header.output_count)
    }

    pub fn required_state_bytes(self) -> usize {
        bit_bytes(self.header.latch_count)
    }

    pub fn required_scratch_bytes(self) -> usize {
        self.header.signal_count() as usize
    }

    pub fn output_signal(self, index: u32) -> Option<u32> {
        if index >= self.header.output_count {
            return None;
        }
        let offset = self.outputs_offset + usize::try_from(index).ok()? * 3;
        read_u24_at(self.bytes, offset)
    }

    pub fn gates(self) -> GateIter<'a> {
        GateIter {
            body: &self.bytes[self.body_offset..],
            cursor: 0,
            remaining: self.header.gate_count,
        }
    }

    pub fn step(
        self,
        input: &[u8],
        state: &[u8],
        scratch: &mut [u8],
        next_state: &mut [u8],
        output: &mut [u8],
    ) -> Result<(), Error> {
        check_exact(
            input.len(),
            self.required_input_bytes(),
            |expected, actual| Error::InputLength { expected, actual },
        )?;
        check_exact(
            state.len(),
            self.required_state_bytes(),
            |expected, actual| Error::StateLength { expected, actual },
        )?;
        check_exact(
            next_state.len(),
            self.required_state_bytes(),
            |expected, actual| Error::StateLength { expected, actual },
        )?;
        check_exact(
            output.len(),
            self.required_output_bytes(),
            |expected, actual| Error::OutputLength { expected, actual },
        )?;
        let required_scratch = self.required_scratch_bytes();
        if scratch.len() < required_scratch {
            return Err(Error::ScratchLength {
                expected_at_least: required_scratch,
                actual: scratch.len(),
            });
        }

        scratch[..required_scratch].fill(0);
        next_state.fill(0);
        output.fill(0);
        scratch[1] = 1;
        for bit in 0..self.header.input_count {
            scratch[(2 + bit) as usize] = u8::from(read_bit(input, bit));
        }

        let mut latch = 0u32;
        for (gate_index, gate) in self.gates().enumerate() {
            let signal = 2 + self.header.input_count + gate_index as u32;
            let value = match gate {
                Gate::Nand { a, b } => !(scratch[a as usize] != 0 && scratch[b as usize] != 0),
                Gate::Latch { .. } => {
                    let value = read_bit(state, latch);
                    latch += 1;
                    value
                }
            };
            scratch[signal as usize] = u8::from(value);
        }

        latch = 0;
        // v1 canonical bytecode stores all LATCH records first, so state commit only scans the
        // small latch prefix rather than the complete combinational graph a second time.
        for gate in self.gates().take(self.header.latch_count as usize) {
            if let Gate::Latch { d } = gate {
                write_bit(next_state, latch, scratch[d as usize] != 0);
                latch += 1;
            }
        }
        for index in 0..self.header.output_count {
            let signal = self
                .output_signal(index)
                .ok_or(Error::TruncatedOutputTable)?;
            write_bit(output, index, scratch[signal as usize] != 0);
        }
        Ok(())
    }

    fn validate_outputs(self, signal_count: u32) -> Result<(), Error> {
        for output in 0..self.header.output_count {
            let signal = self
                .output_signal(output)
                .ok_or(Error::TruncatedOutputTable)?;
            if signal >= signal_count {
                return Err(Error::InvalidOutput { output, signal });
            }
        }
        Ok(())
    }

    fn validate_gates(self, signal_count: u32) -> Result<(), Error> {
        let body = &self.bytes[self.body_offset..];
        let mut cursor = 0usize;
        let mut latches = 0u32;
        let mut saw_nand = false;
        for gate in 0..self.header.gate_count {
            let opcode = *body.get(cursor).ok_or(Error::TruncatedGate { gate })?;
            let current_signal = 2 + self.header.input_count + gate;
            match opcode {
                OP_NAND => {
                    saw_nand = true;
                    if body.len().saturating_sub(cursor) < 7 {
                        return Err(Error::TruncatedGate { gate });
                    }
                    let a = read_u24_at(body, cursor + 1).ok_or(Error::TruncatedGate { gate })?;
                    let b = read_u24_at(body, cursor + 4).ok_or(Error::TruncatedGate { gate })?;
                    if a >= current_signal {
                        return Err(Error::InvalidNandReference { gate, signal: a });
                    }
                    if b >= current_signal {
                        return Err(Error::InvalidNandReference { gate, signal: b });
                    }
                    cursor += 7;
                }
                OP_LATCH => {
                    if saw_nand {
                        return Err(Error::NonCanonicalLatchOrder { gate });
                    }
                    if body.len().saturating_sub(cursor) < 4 {
                        return Err(Error::TruncatedGate { gate });
                    }
                    let d = read_u24_at(body, cursor + 1).ok_or(Error::TruncatedGate { gate })?;
                    if d >= signal_count {
                        return Err(Error::InvalidLatchReference { gate, signal: d });
                    }
                    latches += 1;
                    cursor += 4;
                }
                opcode => return Err(Error::UnknownOpcode { gate, opcode }),
            }
        }
        if cursor != body.len() {
            return Err(Error::LengthMismatch);
        }
        if latches != self.header.latch_count {
            return Err(Error::LatchCountMismatch {
                declared: self.header.latch_count,
                actual: latches,
            });
        }
        Ok(())
    }
}

pub struct GateIter<'a> {
    body: &'a [u8],
    cursor: usize,
    remaining: u32,
}

impl Iterator for GateIter<'_> {
    type Item = Gate;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let opcode = *self.body.get(self.cursor)?;
        let gate = match opcode {
            OP_NAND => {
                let a = read_u24_at(self.body, self.cursor + 1)?;
                let b = read_u24_at(self.body, self.cursor + 4)?;
                self.cursor += 7;
                Gate::Nand { a, b }
            }
            OP_LATCH => {
                let d = read_u24_at(self.body, self.cursor + 1)?;
                self.cursor += 4;
                Gate::Latch { d }
            }
            _ => return None,
        };
        self.remaining -= 1;
        Some(gate)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for GateIter<'_> {}

pub const fn bit_bytes(bits: u32) -> usize {
    (bits as usize).div_ceil(8)
}

pub fn read_bit(bytes: &[u8], bit: u32) -> bool {
    let index = bit as usize;
    bytes
        .get(index / 8)
        .is_some_and(|byte| byte & (1 << (index % 8)) != 0)
}

pub fn write_bit(bytes: &mut [u8], bit: u32, value: bool) {
    let index = bit as usize;
    if let Some(byte) = bytes.get_mut(index / 8) {
        let mask = 1 << (index % 8);
        if value {
            *byte |= mask;
        } else {
            *byte &= !mask;
        }
    }
}

fn check_exact<F>(actual: usize, expected: usize, error: F) -> Result<(), Error>
where
    F: FnOnce(usize, usize) -> Error,
{
    if actual == expected {
        Ok(())
    } else {
        Err(error(expected, actual))
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let raw: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or(Error::TruncatedHeader)?
        .try_into()
        .map_err(|_| Error::TruncatedHeader)?;
    Ok(u32::from_le_bytes(raw))
}

fn read_u24_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let raw = bytes.get(offset..offset + 3)?;
    Some(u32::from(raw[0]) | (u32::from(raw[1]) << 8) | (u32::from(raw[2]) << 16))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use nandry_ir::sha256v;
    use serde::Deserialize;
    use std::vec;

    #[derive(Deserialize)]
    struct GoldenFile {
        format: std::string::String,
        version: u8,
        vectors: std::vec::Vec<GoldenVector>,
    }

    #[derive(Deserialize)]
    struct GoldenVector {
        name: std::string::String,
        bytecode_hex: std::string::String,
        sha256: std::string::String,
        steps: std::vec::Vec<GoldenStep>,
    }

    #[derive(Deserialize)]
    struct GoldenStep {
        input_hex: std::string::String,
        state_hex: std::string::String,
        output_hex: std::string::String,
        next_state_hex: std::string::String,
    }

    fn nand_bytes(a: u32, b: u32) -> std::vec::Vec<u8> {
        let mut bytes = vec![OP_NAND];
        bytes.extend_from_slice(&a.to_le_bytes()[..3]);
        bytes.extend_from_slice(&b.to_le_bytes()[..3]);
        bytes
    }

    fn one_nand(a: u32, b: u32) -> std::vec::Vec<u8> {
        let mut bytes = std::vec::Vec::from(MAGIC);
        bytes.extend_from_slice(&[VERSION, 0, 0, 0]);
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&7u32.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes()[..3]);
        bytes.extend_from_slice(&nand_bytes(a, b));
        bytes
    }

    #[test]
    fn evaluates_nand() {
        let bytes = one_nand(2, 3);
        let circuit = CircuitView::parse(&bytes).unwrap();
        let mut scratch = vec![0; circuit.required_scratch_bytes()];
        let mut output = vec![0; circuit.required_output_bytes()];
        circuit
            .step(&[0b11], &[], &mut scratch, &mut [], &mut output)
            .unwrap();
        assert_eq!(output[0] & 1, 0);

        circuit
            .step(&[0b01], &[], &mut scratch, &mut [], &mut output)
            .unwrap();
        assert_eq!(output[0] & 1, 1);
    }

    #[test]
    fn rejects_forward_nand_reference() {
        assert_eq!(
            CircuitView::parse(&one_nand(4, 3)).err(),
            Some(Error::InvalidNandReference { gate: 0, signal: 4 })
        );
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = one_nand(2, 3);
        bytes.push(0);
        assert_eq!(
            CircuitView::parse(&bytes).err(),
            Some(Error::LengthMismatch)
        );
    }

    #[test]
    fn shared_golden_vectors_match_vm_semantics() {
        let golden: GoldenFile =
            serde_json::from_str(include_str!("../../../golden/vm-v1.json")).unwrap();
        assert_eq!(golden.format, "nandry-tsol");
        assert_eq!(golden.version, VERSION);
        for vector in golden.vectors {
            let bytecode = decode_hex(&vector.bytecode_hex);
            assert_eq!(
                sha256v(&[&bytecode]).as_slice(),
                decode_hex(&vector.sha256),
                "{} digest",
                vector.name
            );
            let circuit = CircuitView::parse(&bytecode)
                .unwrap_or_else(|error| panic!("{} did not parse: {error}", vector.name));
            for step in vector.steps {
                let input = decode_hex(&step.input_hex);
                let state = decode_hex(&step.state_hex);
                let mut scratch = vec![0; circuit.required_scratch_bytes()];
                let mut next_state = vec![0; circuit.required_state_bytes()];
                let mut output = vec![0; circuit.required_output_bytes()];
                circuit
                    .step(&input, &state, &mut scratch, &mut next_state, &mut output)
                    .unwrap();
                assert_eq!(
                    output,
                    decode_hex(&step.output_hex),
                    "{} output",
                    vector.name
                );
                assert_eq!(
                    next_state,
                    decode_hex(&step.next_state_hex),
                    "{} next state",
                    vector.name
                );
            }
        }
    }

    fn decode_hex(value: &str) -> std::vec::Vec<u8> {
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
