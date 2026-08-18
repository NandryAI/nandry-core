#![no_std]

use core::fmt;
use sha2::{Digest, Sha256};

pub const MACHINE_SPEC_DOMAIN: [u8; 8] = *b"NDRYMCH1";
pub const MACHINE_INPUT_DOMAIN: [u8; 8] = *b"NDRYINP1";
pub const MACHINE_OUTPUT_DOMAIN: [u8; 8] = *b"NDRYOUT1";
pub const MACHINE_SPEC_VERSION: u16 = 1;
pub const TSOL_VM_VERSION: u16 = 1;
pub const EXECUTION_MODE_ZERO_STATE_FIXED_INPUT: u8 = 1;
pub const MACHINE_SPEC_V1_BYTES: usize = 64;

pub fn sha256v(chunks: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for chunk in chunks {
        hasher.update(chunk);
    }
    hasher.finalize().into()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineSpecV1 {
    pub circuit_id: [u8; 32],
    pub input_count: u32,
    pub output_count: u32,
    pub gate_count: u32,
    pub latch_count: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineSpecError {
    EmptyInput,
    EmptyOutput,
    InvalidGateCounts,
}

impl fmt::Display for MachineSpecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => formatter.write_str("machine spec requires at least one input bit"),
            Self::EmptyOutput => {
                formatter.write_str("machine spec requires at least one output bit")
            }
            Self::InvalidGateCounts => formatter.write_str("machine spec gate counts are invalid"),
        }
    }
}

impl MachineSpecV1 {
    pub fn new(
        circuit_id: [u8; 32],
        input_count: u32,
        output_count: u32,
        gate_count: u32,
        latch_count: u32,
    ) -> Result<Self, MachineSpecError> {
        if input_count == 0 {
            return Err(MachineSpecError::EmptyInput);
        }
        if output_count == 0 {
            return Err(MachineSpecError::EmptyOutput);
        }
        if gate_count == 0 || gate_count < latch_count {
            return Err(MachineSpecError::InvalidGateCounts);
        }
        Ok(Self {
            circuit_id,
            input_count,
            output_count,
            gate_count,
            latch_count,
        })
    }

    pub fn canonical_bytes(self) -> [u8; MACHINE_SPEC_V1_BYTES] {
        let mut bytes = [0; MACHINE_SPEC_V1_BYTES];
        bytes[..8].copy_from_slice(&MACHINE_SPEC_DOMAIN);
        bytes[8..10].copy_from_slice(&MACHINE_SPEC_VERSION.to_le_bytes());
        bytes[10..12].copy_from_slice(&TSOL_VM_VERSION.to_le_bytes());
        bytes[12] = EXECUTION_MODE_ZERO_STATE_FIXED_INPUT;
        bytes[16..48].copy_from_slice(&self.circuit_id);
        bytes[48..52].copy_from_slice(&self.input_count.to_le_bytes());
        bytes[52..56].copy_from_slice(&self.output_count.to_le_bytes());
        bytes[56..60].copy_from_slice(&self.gate_count.to_le_bytes());
        bytes[60..64].copy_from_slice(&self.latch_count.to_le_bytes());
        bytes
    }

    pub fn machine_id(self) -> [u8; 32] {
        sha256v(&[&self.canonical_bytes()])
    }
}

pub fn machine_input_digest(
    machine_id: [u8; 32],
    input_width: u32,
    ticks: u8,
    input: &[u8],
) -> [u8; 32] {
    sha256v(&[
        &MACHINE_INPUT_DOMAIN,
        &MACHINE_SPEC_VERSION.to_le_bytes(),
        &machine_id,
        &input_width.to_le_bytes(),
        &[ticks],
        input,
    ])
}

pub fn machine_output_digest(
    machine_id: [u8; 32],
    input_digest: [u8; 32],
    output_width: u32,
    output: &[u8],
) -> [u8; 32] {
    sha256v(&[
        &MACHINE_OUTPUT_DOMAIN,
        &MACHINE_SPEC_VERSION.to_le_bytes(),
        &machine_id,
        &input_digest,
        &output_width.to_le_bytes(),
        output,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_spec_v1_is_canonical_and_domain_separated() {
        let spec = MachineSpecV1::new([7; 32], 16, 4, 1_405, 51).unwrap();
        let encoded = spec.canonical_bytes();
        assert_eq!(&encoded[..8], b"NDRYMCH1");
        assert_eq!(encoded.len(), 64);
        assert_eq!(
            spec.machine_id(),
            MachineSpecV1::new([7; 32], 16, 4, 1_405, 51)
                .unwrap()
                .machine_id()
        );
        assert_ne!(
            spec.machine_id(),
            MachineSpecV1::new([7; 32], 16, 4, 1_406, 51)
                .unwrap()
                .machine_id()
        );

        let input = [0b0000_0101, 0];
        let input_digest = machine_input_digest(spec.machine_id(), 16, 2, &input);
        let output_digest =
            machine_output_digest(spec.machine_id(), input_digest, 4, &[0b0000_0010]);
        assert_ne!(input_digest, output_digest);
        assert_ne!(
            input_digest,
            machine_input_digest(spec.machine_id(), 16, 3, &input)
        );
    }

    #[test]
    fn machine_spec_v1_reproducibility_vector_is_stable() {
        let circuit_id = core::array::from_fn(|index| index as u8);
        let spec = MachineSpecV1::new(circuit_id, 16, 4, 1_405, 51).unwrap();
        let machine_id = [
            45, 168, 231, 217, 18, 209, 110, 70, 14, 131, 81, 33, 199, 177, 59, 90, 161, 13, 3,
            183, 42, 23, 32, 73, 239, 188, 198, 134, 144, 167, 141, 49,
        ];
        let input_digest = [
            157, 90, 204, 131, 148, 15, 169, 207, 45, 60, 220, 137, 10, 131, 254, 203, 27, 136,
            133, 205, 213, 9, 218, 58, 232, 64, 59, 46, 75, 117, 106, 125,
        ];
        let output_digest = [
            214, 108, 121, 41, 171, 189, 232, 36, 189, 226, 156, 82, 231, 161, 94, 82, 51, 27, 8,
            130, 88, 241, 134, 255, 27, 209, 61, 185, 140, 242, 95, 153,
        ];

        assert_eq!(spec.machine_id(), machine_id);
        assert_eq!(
            machine_input_digest(machine_id, 16, 2, &[5, 0]),
            input_digest
        );
        assert_eq!(
            machine_output_digest(machine_id, input_digest, 4, &[2]),
            output_digest
        );
    }
}
