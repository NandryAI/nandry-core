use crate::{Builder, Signal};

pub type Word = [Signal; 8];

impl Builder {
    pub fn not(&mut self, value: Signal) -> Signal {
        self.nand(value, value)
    }

    pub fn and(&mut self, a: Signal, b: Signal) -> Signal {
        let nand = self.nand(a, b);
        self.not(nand)
    }

    pub fn or(&mut self, a: Signal, b: Signal) -> Signal {
        let not_a = self.not(a);
        let not_b = self.not(b);
        self.nand(not_a, not_b)
    }

    pub fn xor(&mut self, a: Signal, b: Signal) -> Signal {
        let common = self.nand(a, b);
        let left = self.nand(a, common);
        let right = self.nand(b, common);
        self.nand(left, right)
    }

    pub fn mux(&mut self, select: Signal, when_false: Signal, when_true: Signal) -> Signal {
        let not_select = self.not(select);
        let left = self.nand(when_false, not_select);
        let right = self.nand(when_true, select);
        self.nand(left, right)
    }

    pub fn reduce_or(&mut self, values: &[Signal]) -> Signal {
        values
            .iter()
            .copied()
            .fold(Self::FALSE, |acc, value| self.or(acc, value))
    }

    pub fn eq_const(&mut self, bits: &[Signal], value: u8) -> Signal {
        let matches: Vec<_> = bits
            .iter()
            .enumerate()
            .map(|(bit, signal)| {
                if value & (1 << bit) == 0 {
                    self.not(*signal)
                } else {
                    *signal
                }
            })
            .collect();
        matches
            .into_iter()
            .fold(Self::TRUE, |acc, signal| self.and(acc, signal))
    }

    pub fn mux_word(&mut self, select: Signal, when_false: &Word, when_true: &Word) -> Word {
        core::array::from_fn(|bit| self.mux(select, when_false[bit], when_true[bit]))
    }

    pub fn select_register(&mut self, registers: &[Word; 4], selector: [Signal; 2]) -> Word {
        core::array::from_fn(|bit| {
            let low = self.mux(selector[0], registers[0][bit], registers[1][bit]);
            let high = self.mux(selector[0], registers[2][bit], registers[3][bit]);
            self.mux(selector[1], low, high)
        })
    }

    pub fn add_word(&mut self, a: &Word, b: &Word, carry_in: Signal) -> (Word, Signal) {
        let mut carry = carry_in;
        let mut sum = [Self::FALSE; 8];
        for bit in 0..8 {
            let axb = self.xor(a[bit], b[bit]);
            sum[bit] = self.xor(axb, carry);
            let carry_ab = self.and(a[bit], b[bit]);
            let carry_axb = self.and(carry, axb);
            carry = self.or(carry_ab, carry_axb);
        }
        (sum, carry)
    }

    pub fn sub_word(&mut self, a: &Word, b: &Word) -> (Word, Signal) {
        let inverted = core::array::from_fn(|bit| self.not(b[bit]));
        self.add_word(a, &inverted, Self::TRUE)
    }

    pub fn and_word(&mut self, a: &Word, b: &Word) -> Word {
        core::array::from_fn(|bit| self.and(a[bit], b[bit]))
    }

    pub fn or_word(&mut self, a: &Word, b: &Word) -> Word {
        core::array::from_fn(|bit| self.or(a[bit], b[bit]))
    }

    pub fn xor_word(&mut self, a: &Word, b: &Word) -> Word {
        core::array::from_fn(|bit| self.xor(a[bit], b[bit]))
    }
}
