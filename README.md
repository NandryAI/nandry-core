# NANDRY Core

NANDRY Core is a deterministic compiler and runtime for synchronous circuits
built from NAND gates and one-bit latches.

A circuit graph is compiled into canonical TSOL v1 bytecode. The same Rust VM
validates and executes those bytes on native targets and WebAssembly. Machine
identity, inputs, outputs, and execution traces are committed with versioned,
domain-separated SHA-256 encodings.

## Workspace

| Path | Purpose |
|---|---|
| `crates/nandry-vm` | `no_std`, allocation-free TSOL parser and evaluator |
| `crates/nandry-compiler` | Canonical graph ordering and TSOL encoder |
| `crates/nandry-ir` | Machine Spec v1 and commitment primitives |
| `crates/nandry-trace` | Per-tick trace commitment chain |
| `crates/nandry-wasm` | WebAssembly bindings for the compiler and VM |
| `crates/nandry-circuits` | Gate-level logic library and an 8-bit CPU |
| `golden/vm-v1.json` | Runtime-independent execution vectors |

## Invariants

- Graph node and edge order do not affect compiled bytes.
- Signal numbering is canonical and independent of hash-map iteration order.
- The VM rejects combinational cycles and forward NAND references.
- Every latch reads the old state; all next-state bits commit together after a
  tick.
- Bit vectors are packed least-significant bit first, with zero padding.
- Machine IDs bind the format version, execution mode, circuit digest, and
  circuit dimensions.
- Trace roots bind every committed state and output in order.

## TSOL v1

TSOL is a compact forward netlist. The fixed header is 28 bytes:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | `TSOL` magic |
| 4 | 1 | format version |
| 5 | 1 | flags |
| 6 | 2 | reserved |
| 8 | 4 | input bit count |
| 12 | 4 | output bit count |
| 16 | 4 | gate count |
| 20 | 4 | latch count |
| 24 | 4 | gate body length |

The output table follows as little-endian u24 signal indexes. Gate records are:

```text
NAND  = 0x00 || a:u24 || b:u24
LATCH = 0x01 || d:u24
```

Signals 0 and 1 are constants. Inputs follow, then one signal per gate. Latches
form a prefix of the gate body. NAND operands may only reference earlier
signals; latch inputs may reference any signal in the machine, allowing
sequential feedback without combinational cycles.

See [TSOL v1](docs/tsol-v1.md) and
[Machine Spec v1](docs/machine-spec-v1.md) for the byte-level definitions.

## Gate-level CPU

`nandry-circuits` constructs an 8-bit CPU from the same NAND and latch
primitives accepted by the compiler and VM. Its current netlist contains 1,354
NAND gates and 51 latches, encoded in 9,863 bytes. The examples run Fibonacci,
multiplication, and Euclidean GCD programs through the gate-level machine.

```bash
cargo run -p nandry-circuits --example cpu_report --release
```

## Reproducibility

The compiler uses ordered maps and a deterministic topological traversal.
Tests permute graph declarations and assert identical TSOL bytes. Machine Spec
v1 includes fixed digest vectors, while `golden/vm-v1.json` locks parser and
execution behavior across native Rust and WebAssembly builds.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

rustup target add wasm32-unknown-unknown
cargo build -p nandry-wasm --target wasm32-unknown-unknown --release
```

Rust 1.89.0 is pinned in `rust-toolchain.toml`.

## License

Apache-2.0.
