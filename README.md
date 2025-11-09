# PDF Math Recognition Library

## Goals

- Parse digital PDFs from scratch
- Extract text glyphs and vector paths
- Detect and reconstruct mathematical structures (superscripts, fractions, matrices, radicals)
- Export to MathML or LaTeX

## Building and Running

Build the project:

```bash
cargo build
./target/debug/pdfreader test-data/manuscript-test.pdf
```

Or build and run directly:

```bash
cargo run -- test-data/manuscript-test.pdf
```

Every run flows through a simple `Pipeline` (header → xref → objects) and produces a `.log` file beside the input PDF so you can inspect exactly which stage succeeded/failed before chaining the results into future Markdown/JSON converters.
