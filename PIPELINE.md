# PDF Reader Pipeline Architecture

## Overview

The PDF reader processes files through a **4-stage pipeline** that transforms raw bytes into structured data and extracted text. Each stage is logged and depends on previous stages completing successfully.

```
┌─────────────────────────────────────────────────────────────────┐
│                     Raw PDF File Bytes                          │
└────────────────┬────────────────────────────────────────────────┘
                 │
                 ▼
        ┌────────────────────┐
        │  Stage 1: HEADER   │  Parse PDF version (%PDF-1.3)
        │  parse_header()    │
        └────────┬───────────┘
                 │  output: String (header)
                 ▼
        ┌────────────────────┐
        │  Stage 2: XREF     │  Find and parse cross-reference table
        │ parse_xref_and_    │  Extract startxref offset, parse entries
        │   trailer()        │  Extract trailer dictionary
        └────────┬───────────┘
                 │  output: (XrefTable, HashMap<trailer>)
                 ▼
        ┌────────────────────┐
        │ Stage 3: OBJECTS   │  Parse all PDF objects using xref offsets
        │ objects::parse_    │  Create HashMap: object_id → Object
        │ objects()          │  Objects: Null, Number, String, Name,
        │                    │            Array, Dictionary, Stream, Ref
        └────────┬───────────┘
                 │  output: HashMap<u32, Object>
                 ▼
        ┌────────────────────┐
        │ Stage 4: CONTENT   │  Extract text from page streams
        │ content::extract_  │  • Find page objects
        │ text()             │  • Collect content streams
        │                    │  • Decode stream data (FlateDecode)
        │                    │  • Parse content operators (Tj, TJ, etc)
        │                    │  • Extract text strings
        └────────┬───────────┘
                 │  output: Vec<PageText>
                 ▼
        ┌────────────────────────┐
        │    PdfDocument         │
        ├────────────────────────┤
        │ header: String         │
        │ size: usize            │
        │ xref_table: XrefTable  │
        │ trailer: HashMap       │
        │ objects: HashMap       │
        │ pages: Vec<PageText>   │
        └────────────────────────┘
```

---

## Pipeline Stages in Detail

### Stage 1: Header Parsing

**Function:** `parse_header()` in `src/main.rs:105`

**Task:** Extract PDF version string from file beginning

- Reads first ~10 bytes
- Returns string like `%PDF-1.3`, `%PDF-1.7`, etc.

**Input:** Raw file bytes
**Output:** `String` (header)

---

### Stage 2: XREF & Trailer Parsing

**Function:** `parse_xref_and_trailer()` in `src/main.rs:111`

**Task:** Locate and parse cross-reference table

1. Search backwards for `startxref` keyword (within last 1KB)
2. Extract xref offset value
3. Jump to offset, parse `xref` section
4. Parse trailer dictionary `<< /Root ... >>`

**Returns:**

- `XrefTable` - Maps object number to file offset (via `src/xref.rs`)
- `HashMap<String, String>` - Trailer key-value pairs

**Dependencies:** None (direct byte search)

---

### Stage 3: Objects Parsing

**Function:** `objects::parse_objects()` in `src/objects.rs:50`

**Task:** Parse all PDF objects using xref offsets

1. For each xref entry, jump to offset in file
2. Parse indirect object: `<num> <gen> obj <value> endobj`
3. Recursively parse object value:
   - Scalars: Null, Boolean, Number
   - Strings: Literal `(...)` or hex `<...>`
   - Names: `/Name`
   - Arrays: `[ ... ]`
   - Dictionaries: `<< /key value ... >>`
   - Streams: Dictionary followed by `stream ... endstream`
   - References: `<num> <gen> R`

**Returns:** `HashMap<u32, Object>` - Maps object ID to parsed Object

**Module Dependencies:**

- `src/lexer.rs` - Token parsing (Token enum, Lexer struct)
- `src/objects.rs` - Object types and parsing logic

**Lexer Features:**

- Handles comments (`% ...`)
- Escape sequences in strings (`\n`, `\t`, `\\`, octal, hex)
- Hex string decoding `<48656C6C6F>`
- Stream data reading (exact length or endstream marker)

---

### Stage 4: Content (Text) Extraction

**Function:** `content::extract_text()` in `src/content.rs:19`

**Task:** Walk object tree and extract text from page streams

**Process:**

1. **Find Page Objects** - Iterate all objects, find Type=/Page
2. **Collect Content Streams** - Get Contents field (single ref, array, or inline)
3. **Resolve References** - Follow indirect references recursively
4. **Decode Streams** - Apply /Filter decoders
   - Currently: FlateDecode (zlib via FFI)
   - Phase 2: ASCII85Decode, ASCIIHexDecode, LZWDecode, etc.
5. **Parse Content Operators** - Tokenize and extract text:
   - `Tj` - Show string
   - `TJ` - Show string array
   - `Td`, `TD`, `T*` - Position (insert newline)
   - `'`, `"` - Text operators with operands
   - `BT`/`ET` - Text object begin/end
6. **Accumulate Text** - Build string with proper spacing

**Returns:** `Vec<PageText>` - Vector of page ID + extracted text

**Module Dependencies:**

- `src/stream.rs` - Stream collection & decoding orchestration
- `src/filters.rs` - Filter decoders (FlateDecode)
- `src/content_parser.rs` - Content stream parsing & operator dispatch
- `src/fonts.rs` - Placeholder for Phase 2 font support

---

## Data Flow Through Modules

```
main.rs (Pipeline orchestration)
  ├─> Header stage → parse_header()
  ├─> Xref stage → xref::XrefTable::parse()
  │                    └─> lexer: Token parsing
  ├─> Objects stage → objects::parse_objects()
  │                    ├─> lexer: Tokenization
  │                    └─> objects: Object enum & parsing
  └─> Content stage → content::extract_text()
                      ├─> stream: collect_content_streams()
                      │            └─> resolve_reference()
                      ├─> stream: decode_stream()
                      │            └─> filters: flate_decode()
                      └─> content_parser: parse_content_stream()
                                         ├─> parse_array()
                                         └─> handle_operator()
```

---

## Key Structures

### PdfDocument (Pipeline Output)

```rust
pub struct PdfDocument {
    pub header: String,                          // e.g. "%PDF-1.3"
    pub size: usize,                             // File size in bytes
    pub xref_table: XrefTable,                   // Object offsets
    pub trailer: HashMap<String, String>,        // Metadata
    pub objects: HashMap<u32, Object>,           // All parsed objects
    pub pages: Vec<PageText>,                    // Extracted text per page
}
```

### PageText (Content Output)

```rust
pub struct PageText {
    pub page_id: u32,      // Object ID of the Page
    pub text: String,      // Extracted text content
}
```

### Object Enum

```rust
pub enum Object {
    Null,
    Boolean(bool),
    Number(f64),
    String(Vec<u8>),
    Name(String),
    Array(Vec<Object>),
    Dictionary(HashMap<String, Object>),
    Stream { dict: HashMap<String, Object>, data: Vec<u8> },
    Reference(u32, u16),  // (object_num, generation)
}
```

---

## Error Handling

**Pipeline Error Model:**

- Each stage returns `Result<(), String>`
- If any stage fails, the entire pipeline stops
- All errors are logged to `.log` file
- Process exits with non-zero status

**Logged Output (example):**

```
[INFO] Starting pipeline for file.pdf
[INFO] Read 301494 bytes
[INFO] Stage 'header' started
[INFO] Header parsed: %PDF-1.3
[INFO] Stage 'header' completed
[INFO] Stage 'xref' started
[INFO] Xref entries: 102
[INFO] Stage 'xref' completed
[INFO] Stage 'objects' started
[INFO] Parsed 85 objects
[INFO] Stage 'objects' completed
[INFO] Stage 'content' started
[INFO] Extracted text from 4 pages
[INFO] Stage 'content' completed
[INFO] All pipeline stages completed
[INFO] Pipeline completed successfully
```

---

## Phase 2 Integration Points

Each stage is designed to be extended for Phase 2 deliverables:

### Stage 2: Xref (Nested Trailers)

**File:** `src/xref.rs:130` - `parse_trailer()`

- Currently: Single trailer parsing
- Phase 2: Support multiple xref sections for incremental updates

### Stage 3: Objects (already complete, no changes needed)

### Stage 4a: Stream Decoding (Filter Chains)

**File:** `src/stream.rs:58` - `decode_stream()`

- Currently: Single FlateDecode or single-element array
- Phase 2: Support filter chains like `[/ASCII85Decode /FlateDecode]`

### Stage 4b: Stream Decoding (Indirect Length)

**File:** `src/stream.rs:58` - `decode_stream()`

- Currently: Direct `/Length` extraction
- Phase 2: Follow object references like `/Length 5 0 R`

### Stage 4c: Content Parsing (Font State)

**File:** `src/content_parser.rs:148` - `handle_operator()`

- Currently: Naive byte-to-UTF8 conversion
- Phase 2: Track `Tf` operator, maintain font state

### Stage 4d: Content Parsing (Glyph Mapping)

**File:** `src/content_parser.rs:159` - `append_text()`

- Currently: UTF-8 lossy conversion
- Phase 2: Use font encoding & `/ToUnicode` maps to map glyphs to Unicode

### Stage 5: Exporters (New)

**File:** `src/exporter.rs` (new module)

- Phase 2: Pluggable trait for Markdown, JSON, HTML output
- CLI flag: `--format json|markdown|html`

---

## Running the Pipeline

**Command:**

```bash
cargo run -- <pdf-file>
```

**Output:**

1. Console: Header, xref summary, objects summary, page text preview
2. Log file: `<pdf-file>.log` with full stage execution details

**Example:**

```bash
$ cargo run -- document.pdf
PDF Math Recognition Library
Phase 1: Reading a PDF

File: document.pdf
Detailed log written to document.log
Size: 301494 bytes
Header: %PDF-1.3

✓ xref table parsed
Xref Entries: 102
...

Extracted text (4 page objects):
-- Page object 1 --
This is the extracted text from page 1...
```

---

## Summary

The pipeline is a clean, composable architecture where:

- ✅ Each stage has a single responsibility
- ✅ Dependencies flow left-to-right (header → xref → objects → content)
- ✅ All execution is logged for debugging
- ✅ Error handling is consistent
- ✅ Phase 2 enhancements are clearly marked with TODO comments
- ✅ New stages/exporters can be added without modifying existing code
