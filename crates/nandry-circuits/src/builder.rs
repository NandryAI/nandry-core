use core::fmt;

use nandry_compiler::{encode_netlist, CompileError, Netlist, NetlistGate, MAX_U24};

pub type Signal = u32;

#[derive(Clone, Copy, Debug)]
enum GateSpec {
    Nand { a: Signal, b: Signal },
    Latch { d: Signal },
}

#[derive(Clone, Copy, Debug)]
pub struct LatchHandle {
    gate_index: usize,
    pub q: Signal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildError {
    TooManySignals,
    InvalidSignal(Signal),
    InvalidLatchHandle,
    BodyTooLarge,
    TooManyOutputs,
    NonCanonicalLatchOrder,
    InvalidNetlist,
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for BuildError {}

pub struct Builder {
    input_count: u32,
    inputs: Vec<Signal>,
    gates: Vec<GateSpec>,
}

impl Builder {
    pub const FALSE: Signal = 0;
    pub const TRUE: Signal = 1;

    pub fn new(input_count: u32) -> Result<Self, BuildError> {
        let total = 2u32
            .checked_add(input_count)
            .ok_or(BuildError::TooManySignals)?;
        if total - 1 > MAX_U24 {
            return Err(BuildError::TooManySignals);
        }
        Ok(Self {
            input_count,
            inputs: (2..total).collect(),
            gates: Vec::new(),
        })
    }

    pub fn inputs(&self) -> &[Signal] {
        &self.inputs
    }

    pub fn gate_count(&self) -> u32 {
        self.gates.len() as u32
    }

    pub fn nand(&mut self, a: Signal, b: Signal) -> Signal {
        self.push_gate(GateSpec::Nand { a, b })
    }

    pub fn latch(&mut self, d: Signal) -> LatchHandle {
        let gate_index = self.gates.len();
        let q = self.push_gate(GateSpec::Latch { d });
        LatchHandle { gate_index, q }
    }

    pub fn set_latch_d(&mut self, handle: LatchHandle, d: Signal) -> Result<(), BuildError> {
        match self.gates.get_mut(handle.gate_index) {
            Some(GateSpec::Latch { d: current }) => {
                *current = d;
                Ok(())
            }
            _ => Err(BuildError::InvalidLatchHandle),
        }
    }

    pub fn finish(self, outputs: &[Signal]) -> Result<Vec<u8>, BuildError> {
        let signal_count = 2u32
            .checked_add(self.input_count)
            .and_then(|count| count.checked_add(self.gates.len() as u32))
            .ok_or(BuildError::TooManySignals)?;
        if signal_count - 1 > MAX_U24 {
            return Err(BuildError::TooManySignals);
        }
        for signal in outputs {
            if *signal >= signal_count {
                return Err(BuildError::InvalidSignal(*signal));
            }
        }
        let mut saw_nand = false;
        for gate in &self.gates {
            match gate {
                GateSpec::Nand { .. } => saw_nand = true,
                GateSpec::Latch { .. } if saw_nand => {
                    return Err(BuildError::NonCanonicalLatchOrder);
                }
                GateSpec::Latch { .. } => {}
            }
        }
        let gates = self
            .gates
            .into_iter()
            .map(|gate| match gate {
                GateSpec::Nand { a, b } => NetlistGate::Nand { a, b },
                GateSpec::Latch { d } => NetlistGate::Latch { d },
            })
            .collect();
        encode_netlist(&Netlist {
            input_count: self.input_count,
            outputs: outputs.to_vec(),
            gates,
        })
        .map_err(map_compile_error)
    }

    fn push_gate(&mut self, gate: GateSpec) -> Signal {
        let signal = 2 + self.input_count + self.gates.len() as u32;
        self.gates.push(gate);
        signal
    }
}

fn map_compile_error(error: CompileError) -> BuildError {
    match error {
        CompileError::SignalSpaceExhausted | CompileError::CountOverflow => {
            BuildError::TooManySignals
        }
        CompileError::InvalidOutput { signal, .. }
        | CompileError::InvalidNandReference { signal, .. }
        | CompileError::InvalidLatchReference { signal, .. } => BuildError::InvalidSignal(signal),
        CompileError::NonCanonicalLatchOrder { .. } => BuildError::NonCanonicalLatchOrder,
        CompileError::BodyTooLarge => BuildError::BodyTooLarge,
        _ => BuildError::InvalidNetlist,
    }
}
