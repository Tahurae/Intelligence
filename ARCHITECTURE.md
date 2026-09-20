# Intelligence base-language architecture

Intelligence is a backend-independent categorical base language. It is not an
operating system and does not define kernels, schedulers, drivers, or device
protocols.

The intended dependency direction is:

```text
syntax/parser -> language/type checker -> typed IR -> transformations -> backend
```

`src/language.rs` contains the portable semantic model: spaces, effects,
primitive signatures, untyped flow structure, typed flows, composition, trace,
and gradient validation. It intentionally has no C, CUDA, OpenQASM, OS, or
hardware dependencies.

`src/backend.rs` defines the narrow contract implemented by optional backends.
A backend must prove that it supports a space before lowering a typed flow.
The current C99 emitter in `src/main.rs` remains the bootstrap implementation;
it should be migrated to the `Backend` trait in the next step.

Future operating systems, runtimes, device libraries, and accelerators should
be consumers of this language and its typed IR, not part of the language core.
