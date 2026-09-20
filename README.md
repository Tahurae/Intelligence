# Intelligence Programming Language

Intelligence is a backend-independent categorical base language. The core
specifies spaces, typed flows, composition, products, trace, gradients, effects,
and a typed intermediate representation. Operating systems, runtimes, drivers,
and accelerators are integrations layered on top.

## Build and run

```bash
cargo check
cargo test
cargo run -- program.cat
clang -std=c99 -Wall -Wextra -Werror payload.c -o binary_app
```

The compiler pipeline is:

```text
source -> lexer/parser -> structural AST -> typed IR -> backend validation -> C99 lowering
```

The C99 backend is a portable bootstrap backend. Hardware-specific behavior is
not part of the language core.
