# Phase 1: Reading a PDF with Rust

## Goal

Read and parse a PDF file. Extract the basic structure: header, cross-reference table, and objects.

## Deliverables

- [ ] Parse PDF header (`%PDF-1.x`)
- [ ] Implement basic lexer to tokenize PDF content
- [ ] Parse cross-reference (xref) table
- [ ] Extract trailer dictionary and root object reference
- [ ] Resolve objects via xref offsets
- [ ] Handle basic PDF object types (numbers, names, strings, arrays, dictionaries)

## Steps

1. Set up error handling
2. Implement lexer for PDF tokens
3. Implement xref parser
4. Build object resolver
5. Write tests with a sample PDF

## Next Steps

1. Enhance `xref::parse_trailer` to parse nested trailer dictionaries so Trailer output shows real key/value structure.
2. Resolve indirect `/Length` (and other metadata) references when reading streams to handle objects whose lengths live in separate objects.
3. Add a content-extraction pass that walks page content streams and emits tokens so we can start interpreting text and drawing commands.
