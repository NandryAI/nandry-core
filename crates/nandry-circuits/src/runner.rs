use core::fmt;

use nandry_vm::{read_bit, CircuitView, Error as VmError};

use crate::{
    cpu::{HALT_STATE_OFFSET, OUT_STATE_OFFSET, PC_STATE_OFFSET},
    programs::OUT,
};

#[derive(Debug)]
pub enum RunError {
    Vm(VmError),
    TickLimit { limit: usize },
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RunError {}

impl From<VmError> for RunError {
    fn from(value: VmError) -> Self {
        Self::Vm(value)
    }
}

pub struct CpuMachine<'a> {
    circuit: CircuitView<'a>,
    state: Vec<u8>,
    scratch: Vec<u8>,
    next_state: Vec<u8>,
    output: Vec<u8>,
    ticks: usize,
}

impl<'a> CpuMachine<'a> {
    pub fn new(bytecode: &'a [u8]) -> Result<Self, VmError> {
        let circuit = CircuitView::parse(bytecode)?;
        Ok(Self {
            state: vec![0; circuit.required_state_bytes()],
            scratch: vec![0; circuit.required_scratch_bytes()],
            next_state: vec![0; circuit.required_state_bytes()],
            output: vec![0; circuit.required_output_bytes()],
            circuit,
            ticks: 0,
        })
    }

    pub fn ticks(&self) -> usize {
        self.ticks
    }

    pub fn pc(&self) -> u8 {
        read_byte(&self.state, PC_STATE_OFFSET)
    }

    pub fn output(&self) -> u8 {
        read_byte(&self.state, OUT_STATE_OFFSET)
    }

    pub fn halted(&self) -> bool {
        read_bit(&self.state, HALT_STATE_OFFSET)
    }

    pub fn step(&mut self, program: &[u16]) -> Result<Option<u8>, VmError> {
        if self.halted() {
            return Ok(None);
        }
        let word = program.get(self.pc() as usize).copied().unwrap_or(0x000f);
        let input = word.to_le_bytes();
        self.circuit.step(
            &input,
            &self.state,
            &mut self.scratch,
            &mut self.next_state,
            &mut self.output,
        )?;
        self.state.copy_from_slice(&self.next_state);
        self.ticks += 1;
        if word as u8 & 0xf == OUT {
            Ok(Some(self.output()))
        } else {
            Ok(None)
        }
    }

    pub fn run_until_halt(
        &mut self,
        program: &[u16],
        tick_limit: usize,
    ) -> Result<Vec<u8>, RunError> {
        let mut outputs = Vec::new();
        for _ in 0..tick_limit {
            if self.halted() {
                return Ok(outputs);
            }
            if let Some(value) = self.step(program)? {
                outputs.push(value);
            }
        }
        Err(RunError::TickLimit { limit: tick_limit })
    }

    pub fn run_until_outputs(
        &mut self,
        program: &[u16],
        output_count: usize,
        tick_limit: usize,
    ) -> Result<Vec<u8>, RunError> {
        let mut outputs = Vec::with_capacity(output_count);
        for _ in 0..tick_limit {
            if let Some(value) = self.step(program)? {
                outputs.push(value);
                if outputs.len() == output_count {
                    return Ok(outputs);
                }
            }
        }
        Err(RunError::TickLimit { limit: tick_limit })
    }
}

fn read_byte(bits: &[u8], offset: u32) -> u8 {
    (0..8).fold(0, |value, bit| {
        value | (u8::from(read_bit(bits, offset + bit)) << bit)
    })
}

#[cfg(test)]
mod tests {
    use crate::{build_cpu, programs};

    use super::*;

    #[test]
    fn same_cpu_runs_fibonacci() {
        let cpu = build_cpu().unwrap();
        let mut machine = CpuMachine::new(&cpu.bytecode).unwrap();
        assert_eq!(
            machine
                .run_until_outputs(&programs::fibonacci(), 12, 100)
                .unwrap(),
            [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89]
        );
    }

    #[test]
    fn same_cpu_runs_multiplication() {
        let cpu = build_cpu().unwrap();
        let mut machine = CpuMachine::new(&cpu.bytecode).unwrap();
        let outputs = machine
            .run_until_halt(&programs::multiplication(13, 17), 200)
            .unwrap();
        assert_eq!(outputs, [221]);
    }

    #[test]
    fn same_cpu_runs_gcd() {
        let cpu = build_cpu().unwrap();
        let mut machine = CpuMachine::new(&cpu.bytecode).unwrap();
        let outputs = machine.run_until_halt(&programs::gcd(84, 30), 200).unwrap();
        assert_eq!(outputs, [6]);
    }
}
