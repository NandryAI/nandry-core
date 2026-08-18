use nandry_vm::CircuitView;

use crate::{builder::BuildError, logic::Word, Builder, LatchHandle, Signal};

pub const CPU_INPUT_BITS: u32 = 16;
pub const CPU_STATE_BITS: u32 = 51;
pub const REG_STATE_OFFSET: u32 = 0;
pub const PC_STATE_OFFSET: u32 = 32;
pub const ZERO_STATE_OFFSET: u32 = 40;
pub const CARRY_STATE_OFFSET: u32 = 41;
pub const HALT_STATE_OFFSET: u32 = 42;
pub const OUT_STATE_OFFSET: u32 = 43;

pub struct CpuArtifact {
    pub bytecode: Vec<u8>,
    pub gate_count: u32,
    pub nand_count: u32,
    pub latch_count: u32,
}

pub fn build_cpu() -> Result<CpuArtifact, BuildError> {
    let mut builder = Builder::new(CPU_INPUT_BITS)?;
    let instruction: [Signal; 16] = builder.inputs().try_into().expect("16 CPU input bits");

    let register_latches: [[LatchHandle; 8]; 4] =
        core::array::from_fn(|_| core::array::from_fn(|_| builder.latch(Builder::FALSE)));
    let pc_latches: [LatchHandle; 8] = core::array::from_fn(|_| builder.latch(Builder::FALSE));
    let zero_latch = builder.latch(Builder::FALSE);
    let carry_latch = builder.latch(Builder::FALSE);
    let halt_latch = builder.latch(Builder::FALSE);
    let out_latches: [LatchHandle; 8] = core::array::from_fn(|_| builder.latch(Builder::FALSE));

    let registers: [Word; 4] = register_latches.map(|word| word.map(|latch| latch.q));
    let pc: Word = pc_latches.map(|latch| latch.q);
    let out: Word = out_latches.map(|latch| latch.q);
    let opcode = &instruction[0..4];
    let dst_selector = [instruction[4], instruction[5]];
    let src_selector = [instruction[6], instruction[7]];
    let immediate: Word = instruction[8..16].try_into().expect("8 immediate bits");

    let operations: [Signal; 16] =
        core::array::from_fn(|opcode_value| builder.eq_const(opcode, opcode_value as u8));
    let dst = builder.select_register(&registers, dst_selector);
    let src = builder.select_register(&registers, src_selector);
    let (add, add_carry) = builder.add_word(&dst, &src, Builder::FALSE);
    let (subtract, subtract_carry) = builder.sub_word(&dst, &src);
    let bit_and = builder.and_word(&dst, &src);
    let bit_or = builder.or_word(&dst, &src);
    let bit_xor = builder.xor_word(&dst, &src);

    let mut result = dst;
    for (operation, candidate) in [
        (operations[1], immediate),
        (operations[2], src),
        (operations[3], add),
        (operations[4], subtract),
        (operations[5], bit_and),
        (operations[6], bit_or),
        (operations[7], bit_xor),
    ] {
        result = builder.mux_word(operation, &result, &candidate);
    }
    let write_enable = builder.reduce_or(&operations[1..8]);
    let result_nonzero = builder.reduce_or(&result);
    let result_zero = builder.not(result_nonzero);
    let zero_next = builder.mux(write_enable, zero_latch.q, result_zero);
    let carry_after_add = builder.mux(operations[3], carry_latch.q, add_carry);
    let carry_next = builder.mux(operations[4], carry_after_add, subtract_carry);

    for register in 0..4 {
        let selected = builder.eq_const(&dst_selector, register as u8);
        let write_register = builder.and(write_enable, selected);
        for (bit, result_bit) in result.iter().enumerate() {
            let next = builder.mux(write_register, registers[register][bit], *result_bit);
            builder.set_latch_d(register_latches[register][bit], next)?;
        }
    }

    let one: Word = [
        Builder::TRUE,
        Builder::FALSE,
        Builder::FALSE,
        Builder::FALSE,
        Builder::FALSE,
        Builder::FALSE,
        Builder::FALSE,
        Builder::FALSE,
    ];
    let (incremented_pc, _) = builder.add_word(&pc, &one, Builder::FALSE);
    let not_zero = builder.not(zero_latch.q);
    let not_carry = builder.not(carry_latch.q);
    let jump_zero = builder.and(operations[9], zero_latch.q);
    let jump_nonzero = builder.and(operations[10], not_zero);
    let jump_carry = builder.and(operations[11], carry_latch.q);
    let jump_no_carry = builder.and(operations[12], not_carry);
    let take_branch = builder.reduce_or(&[
        operations[8],
        jump_zero,
        jump_nonzero,
        jump_carry,
        jump_no_carry,
    ]);
    let branched_pc = builder.mux_word(take_branch, &incremented_pc, &immediate);
    let stopped = builder.or(halt_latch.q, operations[15]);
    let pc_next = builder.mux_word(stopped, &branched_pc, &pc);
    for (latch, next) in pc_latches.iter().zip(pc_next) {
        builder.set_latch_d(*latch, next)?;
    }

    let out_next = builder.mux_word(operations[13], &out, &dst);
    for (latch, next) in out_latches.iter().zip(out_next) {
        builder.set_latch_d(*latch, next)?;
    }
    builder.set_latch_d(zero_latch, zero_next)?;
    builder.set_latch_d(carry_latch, carry_next)?;
    builder.set_latch_d(halt_latch, stopped)?;

    let mut outputs = Vec::with_capacity(CPU_STATE_BITS as usize);
    outputs.extend(registers.into_iter().flatten());
    outputs.extend(pc);
    outputs.extend([zero_latch.q, carry_latch.q, halt_latch.q]);
    outputs.extend(out);
    let bytecode = builder.finish(&outputs)?;
    let view = CircuitView::parse(&bytecode).expect("builder must emit valid bytecode");
    let header = view.header();
    Ok(CpuArtifact {
        bytecode,
        gate_count: header.gate_count,
        nand_count: header.gate_count - header.latch_count,
        latch_count: header.latch_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_has_expected_state_shape() {
        let cpu = build_cpu().unwrap();
        let view = CircuitView::parse(&cpu.bytecode).unwrap();
        assert_eq!(view.header().input_count, CPU_INPUT_BITS);
        assert_eq!(view.header().output_count, CPU_STATE_BITS);
        assert_eq!(view.header().latch_count, CPU_STATE_BITS);
        assert_eq!(cpu.nand_count + cpu.latch_count, cpu.gate_count);
    }
}
