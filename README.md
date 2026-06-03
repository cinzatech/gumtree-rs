# gumtree-rs

A Rust implementation of the **SimpleGumTree** AST differencing algorithm
(Falleri & Martinez, ICSE 2024), built on top of [tree-sitter](https://tree-sitter.github.io)
parsers. The CLI mimics the upstream Java tool's `gumtree textdiff -f JSON`
output schema so existing tooling can consume it unchanged.

## Algorithm

Three stages, each implemented in its own module:

1. **`tree` + `ts_convert`** — parse the source with tree-sitter, filter
   anonymous tokens, build an arena-backed internal AST with cached `height`,
   `size`, and a Merkle-style structural `hash`.
2. **`matcher`** — two-phase node matching:
   - `matcher::topdown` — greedy height-ordered anchor matching of isomorphic
     subtrees, with parent-dice tie-breaking for ambiguous candidates.
   - `matcher::bottomup` — for each remaining unmapped node whose descendants
     already anchor, find the best container in T2 by Dice similarity, then
     run the cheap *simple recovery* (kind+label histogram, then parent-
     correspondence) inside the matched pair.
3. **`actions`** — Chawathe edit-script generator producing the six action
   types `insert-tree`, `insert-node`, `delete-tree`, `delete-node`,
   `update-node`, `move-tree`, including an LIS-based alignment pass for
   sibling reorderings.

The `format` module serialises everything to GumTree-compatible JSON.

## Usage

```bash
cargo build --release
./target/release/gumtree-rs textdiff old new -f JSON
```

The output schema matches the Java tool:

```json
{
  "matches": [
    {"src": "Kind: label [start,end]", "dest": "Kind: label [start,end]"}
  ],
  "actions": [
    {"action": "move-tree", "tree": "...", "parent": "...", "at": 2},
    {"action": "update-node", "tree": "...", "label": "new value"}
  ]
}
```

## Testing

```bash
cargo test                       # all tests
cargo test --no-default-features # core lib only, no grammar required
```

Unit tests live alongside each module; behavioural end-to-end tests in
`tests/diff_e2e.rs` construct trees through `TreeBuilder` so they don't
require a grammar.

## Known divergences from Java GumTree

- **Node `kind` strings differ.** GumTree's Java output uses names like
  `YamlTuple`, `YamlHash`; tree-sitter grammars use names like
  `block_mapping_pair`, `block_mapping`. Output is structurally identical
  but lexically different.
- **Position semantics for moves and inserts.** We emit `at` as the final
  index in T2. The Java tool tracks positions dynamically as actions are
  applied; this can change the value of `at` for any single action without
  changing the overall result.
- **No exhaustive optimal recovery.** The "Simple" in SimpleGumTree is exactly
  this trade-off: replace Zhang-Shasha tree-edit-distance with a cheap greedy
  histogram, accepting slightly different (and on average smaller) edit
  scripts.
