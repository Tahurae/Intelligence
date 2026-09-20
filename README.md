# Intelligence Programming Language

Intelligence is a backend-independent categorical base language. The core defines
spaces, typed flows, composition, product wiring, feedback traces, gradients,
and effects without depending on any OS, driver, CUDA runtime, or hardware API.

The current implementation is a normal Rust crate with a portable C99 backend.
The language can be compiled and checked using standard Cargo tooling. This
project is intentionally designed so that device- and OS-specific behavior can
be implemented as separate backends instead of being mixed into the base language.

## Example program

```text
space A = Tensor<128>
space B = Tensor<64>

flow hybrid : (A || A) -> (B || B) = (neural_layer || neural_layer)
flow traced : (A || A) -> (B || B) = ~(neural_layer || neural_layer)
```
