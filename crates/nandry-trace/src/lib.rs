#![no_std]

use core::fmt;
use nandry_ir::{sha256v, MACHINE_SPEC_VERSION};

pub const MACHINE_TRACE_DOMAIN: [u8; 8] = *b"NDRYTRC1";
pub const MACHINE_TRACE_STEP_DOMAIN: [u8; 8] = *b"NDRYSTP1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceError {
    EmptyTrace,
    TooManySteps,
    IncompleteTrace,
}

impl fmt::Display for TraceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTrace => formatter.write_str("machine trace requires at least one step"),
            Self::TooManySteps => formatter.write_str("machine trace received too many steps"),
            Self::IncompleteTrace => formatter.write_str("machine trace is incomplete"),
        }
    }
}

pub struct MachineTraceV1 {
    root: [u8; 32],
    expected_steps: u8,
    next_step: u8,
}

impl MachineTraceV1 {
    pub fn new(
        machine_id: [u8; 32],
        input_digest: [u8; 32],
        expected_steps: u8,
    ) -> Result<Self, TraceError> {
        if expected_steps == 0 {
            return Err(TraceError::EmptyTrace);
        }
        let root = sha256v(&[
            &MACHINE_TRACE_DOMAIN,
            &MACHINE_SPEC_VERSION.to_le_bytes(),
            &machine_id,
            &input_digest,
            &[expected_steps],
        ]);
        Ok(Self {
            root,
            expected_steps,
            next_step: 0,
        })
    }

    pub fn push(&mut self, state: &[u8], output: &[u8]) -> Result<(), TraceError> {
        if self.next_step >= self.expected_steps {
            return Err(TraceError::TooManySteps);
        }
        self.root = sha256v(&[
            &MACHINE_TRACE_STEP_DOMAIN,
            &MACHINE_SPEC_VERSION.to_le_bytes(),
            &self.root,
            &[self.next_step],
            &(state.len() as u32).to_le_bytes(),
            state,
            &(output.len() as u32).to_le_bytes(),
            output,
        ]);
        self.next_step += 1;
        Ok(())
    }

    pub fn finish(self) -> Result<[u8; 32], TraceError> {
        if self.next_step != self.expected_steps {
            return Err(TraceError::IncompleteTrace);
        }
        Ok(self.root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_root_commits_step_order_state_and_output() {
        let machine_id = [
            45, 168, 231, 217, 18, 209, 110, 70, 14, 131, 81, 33, 199, 177, 59, 90, 161, 13, 3,
            183, 42, 23, 32, 73, 239, 188, 198, 134, 144, 167, 141, 49,
        ];
        let input_digest = [
            157, 90, 204, 131, 148, 15, 169, 207, 45, 60, 220, 137, 10, 131, 254, 203, 27, 136,
            133, 205, 213, 9, 218, 58, 232, 64, 59, 46, 75, 117, 106, 125,
        ];
        let expected_root = [
            61, 118, 204, 255, 251, 55, 117, 151, 14, 185, 65, 176, 69, 139, 152, 34, 145, 52, 1,
            177, 94, 184, 107, 61, 91, 245, 220, 75, 142, 132, 204, 192,
        ];
        let mut first = MachineTraceV1::new(machine_id, input_digest, 2).unwrap();
        first.push(&[0], &[1]).unwrap();
        first.push(&[1], &[0]).unwrap();
        let first_root = first.finish().unwrap();
        assert_eq!(first_root, expected_root);

        let mut second = MachineTraceV1::new(machine_id, input_digest, 2).unwrap();
        second.push(&[1], &[0]).unwrap();
        second.push(&[0], &[1]).unwrap();
        assert_ne!(first_root, second.finish().unwrap());
    }
}
