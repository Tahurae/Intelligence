# Intelligence Language & Tool Manual

**Intelligence** is a monoidal, category-theoretic programming language and toolkit designed for compiling and executing `.i` blueprints.

---

## 1. Core CLI Tools

- `intelligence <file.i>`: Compiles and executes Intelligence blueprint files.
- `note "your note text" -t "#tag"`: Saves a timestamped note with a custom tag.
- `notes`: Displays all repository notes saved in `~/.ilang_repository.txt`.
- `notes "#tag"`: Strictly filters notes by tag or keyword (wrap tags in quotes to prevent shell comment interpretation).

---

## 2. Intelligence Language Specification

### Type Declarations
Define core data types and objects within your blueprint space:
type Stream;
type Tensor;

### Morphism Definitions
Morphisms describe typed transformations between objects:
morphism parse: Stream -> Tensor {
    // Transformation logic
}

### Pipeline Composition
Compose data streams and execute operations sequentially using monoidal arrow composition (`>>`):
input_stream >> parse >> process_tensor
