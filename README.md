# Intelligence Programming Language

A practical developer manual for the current repository state.

## Status

This repository is an early prototype for a categorical programming language that tries to represent computation as typed flows between spaces. The code in `src/main.rs` is a Rust-based parser + type checker + C emitter, but it is not yet a complete language runtime or a mature standard library.

The project should be read as a research prototype and a compiler skeleton rather than as a finished language ecosystem.

## What the project is trying to do

The intended design is grounded in a categorical/programming-language model:

- `Space` represents a type or object.
- `Flow` represents a morphism between spaces.
- `Chain` means composition.
- `Parallel` means product / parallel wiring.
- `Feedback` means looped or recursive feedback.
- `PrimitiveRegistry` stores primitive operations and the C statements used to lower them.

The basic conceptual model is:

- primitives carry domain and codomain types
- flows are validated by type matching
- valid flows are lowered to C99
- clang is used to produce a native executable

## Repository layout

Current repository contents include:

- `src/main.rs` — the main Rust implementation
- `README.md` — project documentation/manual
- `install.sh` — installer for building the Rust project into a local binary
- `program.cat` — example categorical language script
- `test.i` — additional prototype input sample
- `payload.c` — generated C output artifact
- `extensions/` — utility-style extension sources
- `std/` — intended standard library directory, currently mostly placeholders
- `Cargo.toml` and `Cargo.lock` — Rust package metadata and lockfile

## Current implementation reality

The codebase is consistent in one important sense: it is a prototype compiler pipeline, not a production language runtime.

At the moment, the repository shows the following:

- The Rust program reads a source file, tokenizes it, parses a flow AST, validates it, and emits C.
- Primitive registration exists for `add` and `mul`.
- A parser and C emitter are present in `src/main.rs`.
- The project is still under active prototype development.
- Several language-level features are not fully implemented yet.
- Some files under `std/` and `extensions/` are empty or placeholder-like.

This means the project is best treated as a compiler experiment with a partial frontend and partial lowering pipeline.

## Installation

Install from the repository root using the provided shell script:

```bash
curl -sSL https://raw.githubusercontent.com/Tahurae/Intelligence/main/install.sh | sh
```

Or install manually from source:

```bash
git clone https://github.com/Tahurae/Intelligence.git
cd Intelligence
cargo build --release
```

If the build succeeds, the binary is typically located in:

```bash
target/release/intelligence
```

You can then run it with a source file:

```bash
./target/release/intelligence program.cat
```

## Current command-line usage

The current Rust entry point expects a file path argument:

```bash
./target/release/intelligence path/to/file.i
```

The program will:

1. read the file
2. tokenize it
3. parse flows
4. validate types against primitives
5. emit a `payload.c` file
6. invoke `clang` to build a native binary

## Example source

The repository contains a sample file:

```text
program.cat
```

It currently contains:

```text
// Universal Categorical Language Script
space Tensor = Array<128>
space Qubit = Quantum<2>
space Organoid = MEA<64>

flow HybridPipeline = (neural_layer || quantum_gate || organoid_pulse) >> ~(neural_layer || quantum_gate || organoid_pulse)
```

This is a conceptual example of the intended design, but it is not yet a fully supported grammar in the current parser implementation. It demonstrates the project’s aspirational direction rather than a stable, fully implemented language surface.

## Syntax model

The intended syntax is based on typed flow composition:

- `space Name = ...` for declarations
- `flow Name = ...` for morphism declarations
- `>>` for sequential composition
- `||` for parallel composition
- `~` for feedback or reset-style operator

Conceptually, a flow might look like:

```text
flow example = add >> mul
```

or:

```text
flow example = (f || g) >> h
```

This is aligned with the code’s design of `Flow::Chain`, `Flow::Parallel`, and `Flow::Feedback`.

## Primitive registry and lowering

The current prototype includes a registry in `src/main.rs` that registers at least:

- `add`
- `mul`

These are treated as typed primitives with a domain and codomain and a corresponding C snippet.

The intended compiler pipeline is:

```text
source program -> lexer -> parser -> validator -> C emitter -> clang -> binary
```

## Current limitations

This repository is intentionally incomplete. The most important limitations are:

- the parser is a prototype, not a robust grammar implementation
- many syntax forms described in the conceptual docs are not yet fully supported
- the stdlib folders are mostly empty placeholders
- extensions are partial utility sources rather than a complete extension framework
- generated output files like `payload.c` are build artifacts, not the canonical source of truth

## Development notes

If you are working on the repo, the key file to study first is:

```text
src/main.rs
```

It contains the parser, flow model, primitive registry, validation logic, and C code emitter.

## Recommended mindset

Treat this project as:

- a language-design prototype
- a compiler pipeline experiment
- a categorical DSL research artifact
- a Rust/C99 bridge for typed flow composition

It is not yet a polished language with a complete standard library and fully mature toolchain.

## Roadmap direction

The project seems to be headed toward:

- richer space declarations
- stronger primitive typing
- more elaborate flow syntax
- an actual standard library under `std/`
- real extension tooling in `extensions/`
- a stable `.i` / `.cat` language surface

## Troubleshooting

If the build fails:

1. verify Rust is installed
2. run `cargo build --release` directly
3. inspect `src/main.rs` for parser and type-checking issues
4. ensure `clang` is installed if the emitter tries to compile generated C

## Summary

The repository is a conceptual and experimental implementation of a typed categorical language that lowers workflows into C99. It is promising, but still early-stage and incomplete. The code and files in the repo are internally consistent with that prototype goal, but they are not yet a finished language product.

## License and contribution

This repository is intended as an experimental project. Contributions should focus on making the parser, type system, and lowering pipeline more coherent and robust.
