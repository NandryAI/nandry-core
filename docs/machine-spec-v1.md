# Machine Spec v1

Machine Spec v1 assigns a stable identity to executable TSOL semantics. All
multi-byte integers are little-endian. Bit vectors use least-significant-bit
first packing.

## Machine ID

The Machine ID is SHA-256 over this 64-byte canonical structure:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | domain separator: `NDRYMCH1` |
| 8 | 2 | Machine Spec version: `1` |
| 10 | 2 | TSOL VM version: `1` |
| 12 | 1 | execution mode: `1` |
| 13 | 3 | reserved, all zero |
| 16 | 32 | SHA-256 of the complete TSOL bytecode |
| 48 | 4 | input bit count |
| 52 | 4 | output bit count |
| 56 | 4 | total gate count |
| 60 | 4 | latch count |

```text
machine_id = SHA256(canonical_machine_spec_v1)
```

The identity binds only executable semantics. The same canonical bytecode and
dimensions always produce the same Machine ID.

## Execution mode 1

Execution mode 1 is `ZERO_STATE_FIXED_INPUT`:

1. Every latch begins at zero.
2. The packed input remains constant for every tick.
3. A tick evaluates all gate outputs from the current latch state.
4. All next-state latch bits commit simultaneously after evaluation.
5. The output of the final requested tick is the machine output.

Unused high bits in the final input, state, and output bytes must be zero.

## Input commitment

```text
SHA256(
  "NDRYINP1"
  || spec_version:u16
  || machine_id:[u8;32]
  || input_width:u32
  || ticks:u8
  || packed_input
)
```

## Output commitment

```text
SHA256(
  "NDRYOUT1"
  || spec_version:u16
  || machine_id:[u8;32]
  || input_digest:[u8;32]
  || output_width:u32
  || packed_output
)
```

## Trace commitment

The initial trace root binds the run identity:

```text
root_0 = SHA256(
  "NDRYTRC1"
  || spec_version:u16
  || machine_id:[u8;32]
  || input_digest:[u8;32]
  || ticks:u8
)
```

Each tick commits the new latch state and the output observed for that tick:

```text
root_(i+1) = SHA256(
  "NDRYSTP1"
  || spec_version:u16
  || root_i:[u8;32]
  || i:u8
  || state_byte_length:u32
  || packed_next_state
  || output_byte_length:u32
  || packed_output
)
```

A trace is complete only after exactly the declared number of steps.

## Reproducibility vector

Given a circuit digest containing bytes `00 01 02 ... 1f`, 16 inputs, 4
outputs, 1,405 gates, and 51 latches:

```text
machine_id    = 2da8e7d912d16e460e835121c7b13b5aa10d03b72a172049efbcc68690a78d31
input(2 ticks)= 0500
input_digest  = 9d5acc83940fa9cf2d3cdc890a83fecb1b8885cdd509da3ae8403b2e4b756a7d
output        = 02
output_digest = d66c7929abbde824bde29c52e7a15e52331b088258f186ff1bd13db98cf25f99
```

These values are fixed by tests in `nandry-ir`. Trace initialization and step
folding are fixed by tests in `nandry-trace`.
