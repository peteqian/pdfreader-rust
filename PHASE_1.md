# Phase 1: Reading a PDF with Rust

## Goal

Read and parse a PDF file. Extract the basic structure: header, cross-reference table, and objects.

## Deliverables

- [x] Parse PDF header (`%PDF-1.x`)
- [x] Implement basic lexer to tokenize PDF content
- [x] Parse cross-reference (xref) table
- [x] Extract trailer dictionary and root object reference
- [x] Resolve objects via xref offsets
- [x] Handle basic PDF object types (numbers, names, strings, arrays, dictionaries)
- [x] Add a content-extraction pass that walks page content streams and emits tokens to interpret text/drawing commands.

## Steps

1. Set up error handling
2. Implement lexer for PDF tokens
3. Implement xref parser
4. Build object resolver
5. Write tests with a sample PDF
