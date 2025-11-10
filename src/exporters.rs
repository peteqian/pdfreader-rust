use crate::pipeline::PdfDocument;

/// Trait for exporting PDF documents in different formats.
///
/// Implement this trait to add support for new export formats.
pub trait Exporter {
    /// Exports the PDF document to a formatted string.
    fn export(&self, pdf: &PdfDocument) -> String;
}

/// Exports PDF content as plain text (just the extracted text, no metadata).
pub struct PlainTextExporter;

impl Exporter for PlainTextExporter {
    fn export(&self, pdf: &PdfDocument) -> String {
        let mut output = String::new();

        for page in &pdf.pages {
            output.push_str(&page.text);
            output.push_str("\n\n--- Page Break ---\n\n");
        }

        output
    }
}

/// Exports PDF content as Markdown with metadata and structure.
pub struct MarkdownExporter;

impl Exporter for MarkdownExporter {
    fn export(&self, pdf: &PdfDocument) -> String {
        let mut output = String::new();

        // Add header metadata
        output.push_str("# PDF Document\n\n");
        output.push_str("## Metadata\n\n");
        output.push_str(&format!("- **File Size**: {} bytes\n", pdf.size));
        output.push_str(&format!("- **PDF Version**: {}\n", pdf.header));
        output.push_str(&format!("- **Total Objects**: {}\n", pdf.objects.len()));
        output.push_str(&format!("- **Total Pages**: {}\n\n", pdf.pages.len()));

        // Add page content
        output.push_str("## Content\n\n");

        for (idx, page) in pdf.pages.iter().enumerate() {
            output.push_str(&format!("### Page {}\n\n", idx + 1));
            output.push_str(&page.text);
            output.push_str("\n\n");
        }

        output
    }
}

/// Exports PDF content as JSON with metadata and structured data.
pub struct JsonExporter;

impl Exporter for JsonExporter {
    fn export(&self, pdf: &PdfDocument) -> String {
        let mut output = String::from("{\n");

        // Add metadata
        output.push_str("  \"metadata\": {\n");
        output.push_str(&format!("    \"fileSize\": {},\n", pdf.size));
        output.push_str(&format!("    \"pdfVersion\": \"{}\",\n", escape_json_string(&pdf.header)));
        output.push_str(&format!("    \"totalObjects\": {},\n", pdf.objects.len()));
        output.push_str(&format!("    \"totalPages\": {}\n", pdf.pages.len()));
        output.push_str("  },\n");

        // Add pages
        output.push_str("  \"pages\": [\n");
        for (idx, page) in pdf.pages.iter().enumerate() {
            output.push_str("    {\n");
            output.push_str(&format!("      \"pageNumber\": {},\n", idx + 1));
            output.push_str(&format!("      \"pageId\": {},\n", page.page_id));
            output.push_str(&format!("      \"content\": \"{}\"\n", escape_json_string(&page.text)));
            output.push_str("    }");

            if idx < pdf.pages.len() - 1 {
                output.push(',');
            }
            output.push('\n');
        }
        output.push_str("  ]\n");

        output.push('}');
        output
    }
}

/// Escapes special characters in strings for JSON output.
fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Factory function to create an exporter by format name.
///
/// # Arguments
/// * `format` - The format name: "json", "markdown", or "plaintext"
///
/// # Returns
/// A boxed trait object implementing the `Exporter` trait.
///
/// # Panics
/// Panics if the format name is not recognized.
pub fn create_exporter(format: &str) -> Box<dyn Exporter> {
    match format.to_lowercase().as_str() {
        "json" => Box::new(JsonExporter),
        "markdown" | "md" => Box::new(MarkdownExporter),
        "plaintext" | "text" | "txt" => Box::new(PlainTextExporter),
        _ => panic!(
            "Unknown export format: '{}'. Supported formats: json, markdown, plaintext",
            format
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_json_string() {
        assert_eq!(escape_json_string("hello"), "hello");
        assert_eq!(escape_json_string("hello\nworld"), "hello\\nworld");
        assert_eq!(escape_json_string("hello\"world"), "hello\\\"world");
        assert_eq!(escape_json_string("hello\\world"), "hello\\\\world");
    }

    #[test]
    fn test_create_exporter() {
        let _json = create_exporter("json");
        let _md = create_exporter("markdown");
        let _txt = create_exporter("plaintext");
    }
}
