# Phase 2: Stream Decoding & Text Export

## Goal

Unlock richer stream decoding and structured text export so downstream consumers can use the extracted content (Markdown/JSON) with accurate glyph mapping.

## Deliverables

- [ ] Resolve `/Filter` arrays, nested trailer dictionaries, and indirect `/Length` references so stream decoding and document metadata work across common pipeline variants.
- [ ] Respect the current text font (`Tf`) and `/Resources` dictionaries to convert glyph codes to Unicode instead of assuming literal bytes.
- [ ] Replace the `print_page_text` preview with pluggable exporters (e.g., Markdown, JSON) driven by the new text extraction output.

## Steps

1. Extend stream parsing to follow indirect values in dictionaries, iterate `/Filter` arrays, and reuse the logic to read nested trailer dictionaries for faithful metadata output.
2. Teach the content parser to maintain text state (current font, encoding, `ToUnicode` maps) so `extract_text` can emit real Unicode strings.
3. Introduce an exporter trait for page text and wire up Markdown + JSON implementations; update the CLI/`main.rs` to select an exporter and remove the hard-coded preview.
