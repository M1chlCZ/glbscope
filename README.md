# glbscope

Inspect GLB files and fail a build when a model exceeds an asset budget.

## What it does

`glbscope` reads a GLB container, parses the JSON chunk, and reports:

- file size, JSON chunk size, BIN chunk size
- node, mesh, primitive, accessor, material, texture, image, animation and scene counts
- vertex and triangle counts
- embedded image bytes
- deepest node hierarchy level
- combined bounding box from the POSITION accessors

With `--budget` it compares the report with limits and exits with code 1 when a
limit breaks.

## Install

```
cargo install glbscope
```

Go 1.85 or later of the Rust toolchain is required.

## Usage

```
glbscope models/hero.glb
glbscope --budget budget.toml public/models
glbscope --json public/models > report.json
```

Directories are walked recursively for `.glb` files. The output is one line per
file plus a summary. Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | No findings. |
| 1 | At least one budget finding. |
| 2 | A file could not be read or parsed, or the arguments are invalid. |

## Budget file

```toml
max_bytes = 2_000_000
max_triangles = 120_000
max_vertices = 80_000
max_nodes = 400
max_materials = 12
max_textures = 8
max_texture_bytes = 4_000_000
allowed_extensions = ["KHR_materials_unlit", "KHR_texture_transform"]
required_extensions = ["KHR_materials_unlit"]
```

Every key is optional. `allowed_extensions` rejects any extension that is not
listed. `required_extensions` rejects a file that misses a listed extension.

## Library

```rust
let report = glbscope::inspect(&bytes)?;
let findings = glbscope::check(&report, &budget);
```

`Report`, `Budget` and `Finding` implement `serde::Serialize`.

## Continuous integration

```yaml
- run: cargo install glbscope --locked
- run: glbscope --budget assets/budget.toml public/models
```

## Development

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## License

MIT. See `LICENSE`.
