# Zest 🍋

Zest is an early-stage Rust GUI engine project. The intended architecture combines
an immediate user API, a persistent internal ECS scene and direct GPU rendering
through `wgpu-hal`, targeting desktop and mobile applications and tools.

**Current implementation is headless.** It provides scoped widget identities,
validation of complete scene declarations before mutation, persistent ECS nodes
and interaction flags. There is no window, layout engine, text renderer or GPU
backend yet. See [plan.md](plan.md) for the full roadmap and acceptance criteria.

## Run

Install Rust 1.95 or newer, then run from this repository root:

```sh
cargo run -p zest-headless
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The demo reorders keyed buttons while retaining their scene identities.
`Scene::update` currently declares the whole scene; partial rebuilds and automatic
call-site keys are planned. Explicit keys are required in this initial API.

## Structure

- Root package `zest`: public facade.
- `crates/zest-core`: private ECS implementation and public scene API.
- `demos/headless`: runnable reconciliation example.
- `examples`: external reference checkouts, excluded from the workspace.

The initial implementation forbids unsafe code. The future HAL crate will have
its own narrowly scoped unsafe policy and documented invariants.

## License

Licensed under Apache-2.0; see [LICENSE](LICENSE).
