<div align="center">
  <img alt="gray-context" src="assets/icon.svg" width="120" height="120" />
  <h1>gray-context</h1>
  <p><strong>One sidecar for the context window: keep it in budget, then fill it with the right text.</strong></p>
  <p>
    <a href="https://gray.alignment.id">Website</a> ·
    <a href="https://github.com/vstaln/gray-context">Source</a> ·
    <a href="https://github.com/vstaln/gray">gray</a>
  </p>
  <p>
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-1c1c20?style=flat-square&labelColor=0a0a0b" /></a>
    <a href="https://www.rust-lang.org"><img alt="Built with Rust" src="https://img.shields.io/badge/built%20with-rust-1c1c20?style=flat-square&labelColor=0a0a0b&logo=rust&logoColor=d4a373" /></a>
    <img alt="gray plugin" src="https://img.shields.io/badge/gray-plugin-1c1c20?style=flat-square&labelColor=0a0a0b&color=7aa2f7" />
  </p>
</div>

<br/>

```bash
gray plugin install context
```

`gray-context` merges ten micro-plugins that manage what goes into the prompt
and how much of it there is into a single sidecar process. Each module keeps
its own `~/.gray/<orig>/` state files and slash commands — existing setups
keep working after you uninstall the standalone versions.

## Replaces

| Module | Former plugin | Hook | Commands |
|---|---|---|---|
| truncate | `gray-truncate` | `tool/after` | `/truncate` |
| compact | `gray-compact` | `context/build` | `/compact` |
| cacheopt | `gray-cacheopt` | `prompt/context`, `context/build` | `/cachestats` |
| header | `gray-header` | `prompt/context` | `/header` |
| context-pin | `gray-context-pin` | `prompt/context` | `/pin` `/unpin` `/pins` |
| resources | `gray-resources` | `context/build` | `/resources` |
| rules | `gray-rules` | `agent/before_start` | `/rules` |
| include | `gray-include` | `prompt/context` | `/include` |
| dirty-guard | `gray-dirty-guard` | `prompt/context` | `/dirty` |
| gh-issues | `gray-gh-issues` | `input/submit` | `/ghissues` |

`prompt/context` fans out to header + pin + include + dirty + cacheopt and
joins their `{text}` results. `context/build` chains compact → resources →
cacheopt, so the summary and the pinned files land in the list that cacheopt
then fingerprints.

There is no overview command: the gray core owns `/context`.

## Build from source

```sh
cargo build --release
gray plugin check ./target/release/context
```

## License

MIT
