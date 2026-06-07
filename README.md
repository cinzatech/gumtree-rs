# diffame

**A diff tool that understands your code.**

Most diff tools compare files line by line. `diffame` parses your code into an
AST with [tree-sitter](https://tree-sitter.github.io), matches nodes
structurally, and produces a minimal edit script that tells you *what changed*,
not *which lines*.

Move a function 200 lines down? One `move-tree` action instead of walls of
red and green. Rename a variable? A single `update-node`, instead of a scattered
shotgun blast across the file.

## Install

```bash
cargo install --path .
```

Or with `make`:

```bash
make build
sudo make install
```

## Quick start

```bash
diffame old.rs new.rs                   # side-by-side terminal output
diffame old.py new.py -f TEXT           # compact text summary
diffame config.json data.json -f JSON   # machine-readable JSON
```

Language is auto-detected from the file extension. For extensionless files
(`Dockerfile`, `Makefile`, …) detection falls back to the filename. Use `-l`
to override:

```bash
diffame a.txt b.txt -l py              # treat as Python
```

## Use as `git diff`

Drop `diffame` in as your external diff driver and get structural diffs
everywhere git shows you a diff:

```bash
git config --global diff.external diffame
```

That's it. `git diff`, `git show`, `git log -p` — all structural now.

To use it selectively:

```bash
GIT_EXTERNAL_DIFF=diffame git diff
```

## Supported languages

60+ languages out of the box, including C, C++, C#, CSS, Dart, Elixir,
Elm, Erlang, Fortran, GDScript, GLSL, Go, GraphQL, Groovy, Haskell, HCL,
HTML, Java, JavaScript, JSON, Julia, Kotlin, LaTeX, Lua, Makefile, Markdown,
Nix, Objective-C, OCaml, Pascal, Perl, PHP, PowerShell, Prolog, Protocol
Buffers, Python, R, Racket, Ruby, Rust, Scala, Scheme, Solidity, SQL, Swift,
TOML, TypeScript, Verilog, XML, YAML, Zig, and more.

Files with unrecognised extensions gracefully fall back to line-level
diffing, `diffame` never refuses work.

## Output formats

| Flag      | Description |
|-----------|-------------|
| `-f SIDE` | Side-by-side coloured terminal output *(default)* |
| `-f TEXT` | Compact text summary of actions |
| `-f JSON` | Machine-readable JSON with full match and action data |

## Options

| Flag | Description |
|------|-------------|
| `-l EXT` | Override language detection (e.g. `rs`, `py`, `js`) |
| `--max-file-size N` | Max input size in bytes (default: 100 MB, `0` = unlimited) |
| `--parse-timeout N` | Parser timeout in seconds (default: 60, `0` = unlimited) |

## How it works

`diffame` implements the SimpleGumTree algorithm (Falleri & Martinez, ICSE 2024)
in three phases:

1. **Parse**: tree-sitter builds a concrete syntax tree for each file.
2. **Match**: a top-down pass anchors identical subtrees; a bottom-up pass
   recovers container-level matches via Dice similarity.
3. **Edit script**: a Chawathe-style generator emits the minimal set of
   `insert`, `delete`, `update`, and `move` actions.

## Development

```bash
make check       # fmt + clippy + tests
make test        # tests only
```

## License

See [LICENSE.md](LICENSE.md).
