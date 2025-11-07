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
