pub const NOP: u8 = 0x0;
pub const LDI: u8 = 0x1;
pub const MOV: u8 = 0x2;
pub const ADD: u8 = 0x3;
pub const SUB: u8 = 0x4;
pub const AND: u8 = 0x5;
pub const OR: u8 = 0x6;
pub const XOR: u8 = 0x7;
pub const JMP: u8 = 0x8;
pub const JZ: u8 = 0x9;
pub const JNZ: u8 = 0xa;
pub const JC: u8 = 0xb;
pub const JNC: u8 = 0xc;
pub const OUT: u8 = 0xd;
pub const NOP2: u8 = 0xe;
pub const HLT: u8 = 0xf;

pub const fn instruction(opcode: u8, dst: u8, src: u8, immediate: u8) -> u16 {
    (opcode as u16 & 0xf)
        | ((dst as u16 & 0x3) << 4)
        | ((src as u16 & 0x3) << 6)
        | ((immediate as u16) << 8)
}

pub const fn ldi(dst: u8, immediate: u8) -> u16 {
    instruction(LDI, dst, 0, immediate)
}

pub const fn binary(opcode: u8, dst: u8, src: u8) -> u16 {
    instruction(opcode, dst, src, 0)
}

pub const fn branch(opcode: u8, target: u8) -> u16 {
    instruction(opcode, 0, 0, target)
}

pub const fn output(register: u8) -> u16 {
    instruction(OUT, register, 0, 0)
}

pub fn fibonacci() -> Vec<u16> {
    vec![
        ldi(0, 0),
        ldi(1, 1),
        output(0),
        binary(MOV, 2, 0),
        binary(ADD, 2, 1),
        binary(MOV, 0, 1),
        binary(MOV, 1, 2),
        branch(JMP, 2),
    ]
}

pub fn multiplication(a: u8, b: u8) -> Vec<u16> {
    vec![
        ldi(0, a),
        ldi(1, b),
        ldi(2, 0),
        ldi(3, 1),
        binary(MOV, 1, 1),
        branch(JZ, 9),
        binary(ADD, 2, 0),
        binary(SUB, 1, 3),
        branch(JMP, 4),
        output(2),
        instruction(HLT, 0, 0, 0),
    ]
}

pub fn gcd(a: u8, b: u8) -> Vec<u16> {
    vec![
        ldi(0, a),
        ldi(1, b),
        binary(MOV, 2, 0),
        binary(SUB, 2, 1),
        branch(JZ, 12),
        branch(JNC, 8),
        binary(MOV, 0, 2),
        branch(JMP, 2),
        binary(MOV, 2, 1),
        binary(SUB, 2, 0),
        binary(MOV, 1, 2),
        branch(JMP, 2),
        output(0),
        instruction(HLT, 0, 0, 0),
    ]
}
