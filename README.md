# ILang (Ingenious) — Categorical Programming Language & Universal System Blueprint

> **A Monoidal Category Language (`.i`) lowering string diagrams to C99 for zero-dependency binary execution.**

---

## 1. The Universal Mental Model: How to Build Any Project

Most traditional software engineering treats systems as imperative sequences of instructions or object hierarchies. **`ilang` models any real-world problem as a String Diagram in a Strict Monoidal Category.**

To construct *any* system—whether a terminal editor, a database, a physics engine, or a neural network—follow this 4-step conceptual framework:

```
       [ Input Wire A ]          [ Input Wire B ]
              │                         │
              ▼                         ▼
         ┌───────────────────────────────────┐
         │   Parallel Tensor ( A ⊗ B )       │
         └───────────────────────────────────┘
                          │
                          ▼  (Sequential Composition: >>)
         ┌───────────────────────────────────┐
         │       Morphism Transformation     │
         └───────────────────────────────────┘
                          │
                          ▼
                  [ Output Wire C ]
```

1. **Identify the Wires (Objects):** Define the static data types entering and leaving your system (e.g., Raw Keypresses, Text Buffers, Disk Storage, Memory Pointers).
2. **Identify the Boxes (Morphisms):** Break down operations into pure functions that transform input wires into output wires.
3. **Connect Wires via Composition:**
   - **Sequential Composition (`>>`):** Feed the output of Box 1 directly into the input of Box 2.
   - **Parallel Tensor Composition (`||`):** Run Box 1 and Box 2 side-by-side without mutual interference.
4. **Lower to Substrate:** Map high-level categorical flows to ultra-fast, native C99 execution routines.

---

## 2. Complete Language Specification

### Core Operators

| Operator | Categorical Concept | Meaning / Execution |
| :--- | :--- | :--- |
| `>>` | Sequential Composition | Chains output of left morphism into input of right morphism. |
| `||` | Tensor Product | Executes left and right transformations concurrently/side-by-side. |
| `~` | Adjoint / Unbind | Dual operation: unbinds matrix bindings or releases tensor state. |

### Syntax & Grammar

#### 1. Wire (Type) Declaration
```ilang
type KeyStream;
type BufferState;
type DiskLog;
```

#### 2. Tensor Product Parsing
Group multi-wire concurrent states using `(` and `)` with `||`:
```ilang
(KeyStream || BufferState)
```

#### 3. Morphism Binding
Define transformations over wires:
```ilang
morphism ProcessKey : (KeyStream || BufferState) -> BufferState;
morphism PersistState : BufferState -> DiskLog;
```

#### 4. System Pipeline Construction
Combine morphisms sequentially and in parallel:
```ilang
pipeline MainEditor = ProcessKey >> PersistState;
```

#### 5. Outer-Product Matrix Binding & Adjoint Unbinding
Bind state space matrices and unbind adjoints for memory cleanup:
```ilang
bind MatrixSpace = BufferState * DiskLog;
unbind ~MatrixSpace;
```

---

## 3. Architecture: The Frozen Core

`ilang` enforces a **Frozen Core Principle**:
- **Compiler Core (`src/`):** Immutable Rust-based compiler handling parsing, monoidal category typechecking, string diagram optimization, and C99 code generation.
- **Substrate Extensions (`extensions/`):** Project-specific low-level C files (`.c`) providing terminal drivers, hardware access, or OS primitives.
- **Modules (`std/` & `examples/`):** Reusable categorical logic (`.i`) written directly in `ilang`.

```text
   .i Source Code  ──►  ilang Compiler  ──►  Generated C99  ──►  gcc/clang  ──►  Native Binary
  (Categorical)         (Frozen Core)        (payload.c)        (Host CC)      (Zero Rust Req)
```

---

## 4. End-to-End Walkthrough: Building the `note` Terminal Editor

### Step 1: Define C Substrates (`extensions/`)
- `editor_nano.c`: Provides ANSI raw-mode termios terminal handling and key capture.
- `persistence_disk.c`: Provides append-only record logging to `~/.ilang_repository.txt`.

### Step 2: Write Categorical Flow (`examples/note_memory.i`)
```ilang
// Connect raw terminal input wire with memory persistence wire
type TerminalKey;
type DiskRecord;

morphism CaptureInput : TerminalKey -> DiskRecord;
morphism WriteRepository : DiskRecord -> DiskRecord;

pipeline NoteEngine = CaptureInput >> WriteRepository;
```

### Step 3: Compile and Lower
```bash
ilang examples/note_memory.i -o note
```

---

## 5. Installation & Usage

### Universal One-Line Install
Installs the pre-compiled native binary directly without needing Rust or Cargo:

```bash
curl -sSL https://raw.githubusercontent.com/Tahurae/ingenious/main/install.sh | sh
```

### Building from Source
```bash
git clone https://github.com/Tahurae/ingenious.git
cd ingenious
cargo build --release
```
