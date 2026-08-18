# TSOL v1

TSOL v1 is the canonical binary representation of a synchronous NAND/LATCH
machine. All multi-byte integers are little-endian.

## Layout

```text
header || output_table || gate_body
```

The header is exactly 28 bytes:

| Offset | Size | Type | Field |
|---:|---:|---|---|
| 0 | 4 | bytes | ASCII `TSOL` |
| 4 | 1 | u8 | version, exactly `1` |
| 5 | 1 | u8 | flags, exactly `0` |
| 6 | 2 | u16 | reserved, exactly `0` |
| 8 | 4 | u32 | input bit count |
| 12 | 4 | u32 | output bit count |
| 16 | 4 | u32 | gate count |
| 20 | 4 | u32 | latch count |
| 24 | 4 | u32 | gate body byte length |

The output table contains `output_count` little-endian u24 signal indexes.

The gate body contains exactly `gate_count` variable-length records:

```text
NAND  = 0x00 || a:u24 || b:u24
LATCH = 0x01 || d:u24
```

The encoded byte length must be exactly:

```text
28 + 3 * output_count + body_len
```

Trailing bytes are invalid.

## Signal space

Signals are numbered as follows:

```text
0                         constant false
1                         constant true
2 .. 2 + input_count - 1  inputs
2 + input_count ..         one signal per gate record
```

The highest signal index must fit in u24.

Every output table entry must reference a valid signal. A NAND at gate index
`g` may reference only signals below `2 + input_count + g`. A latch may
reference any valid signal, including a later NAND signal, because latch inputs
are sampled only when the next state is committed.

All latch records form a prefix of the gate body. The declared latch count must
equal the length of that prefix. This ordering gives each latch a canonical
state-bit index and keeps state updates independent of combinational traversal.

## Tick semantics

For each tick:

1. Initialize signals 0 and 1 to false and true.
2. Copy packed input bits into the input signal range.
3. Evaluate gate records in order. A latch emits its current state bit; a NAND
   emits `!(a && b)`.
4. Read every latch input from the completed signal array into a separate
   next-state vector.
5. Read output signals into the packed output vector.
6. Commit the complete next-state vector simultaneously.

Input, state, and output vectors are packed least-significant bit first. When a
bit count is not divisible by eight, unused high bits in the final byte are
zero.

## Canonical graph compilation

The reference compiler derives TSOL bytes from logical graph structure:

- inputs, outputs, and latches are ordered by node identifier;
- NAND nodes use a deterministic topological order with node identifier as the
  tie-breaker;
- latch records precede NAND records;
- ports are fixed as `out` for sources, `a` and `b` for NAND inputs, `d` for a
  latch input, and `in` for an output node.

Reordering graph declarations or connections therefore does not change the
encoded byte sequence.

## Rejection conditions

A decoder rejects at least the following:

- truncated headers, tables, or gate records;
- wrong magic, unsupported version, non-zero flags, or non-zero reserved bytes;
- integer overflow or signal indexes outside u24;
- a declared length that differs from the physical byte length;
- invalid output or gate references;
- unknown opcodes;
- a latch after the first NAND;
- a latch count different from the canonical prefix length;
- input, state, output, or scratch buffers with invalid lengths.
