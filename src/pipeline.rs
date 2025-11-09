use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use pdfreader::objects::Object;
use pdfreader::xref::XrefTable;

pub struct PdfDocument {
    pub header: String,
    pub size: usize,
    pub xref_table: XrefTable,
    pub trailer: HashMap<String, String>,
    pub objects: HashMap<u32, Object>,
}

pub struct PipelineLog {
    entries: Vec<String>,
}

impl PipelineLog {
    pub fn new() -> Self {
        PipelineLog {
            entries: Vec::new(),
        }
    }

    pub fn info(&mut self, message: impl Into<String>) {
        self.entries.push(format!("[INFO] {}", message.into()));
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.entries.push(format!("[ERROR] {}", message.into()));
    }

    fn write_to(&self, path: &Path) -> io::Result<()> {
        let mut file = File::create(path)?;
        for line in &self.entries {
            writeln!(file, "{}", line)?;
        }
        Ok(())
    }
}

pub struct Pipeline<'a, 'log> {
    stages: Vec<PipelineStage>,
    state: PipelineState<'a>,
    log: &'log mut PipelineLog,
}

struct PipelineStage {
    name: &'static str,
    action: StageAction,
}

type StageAction = Box<dyn Fn(&mut PipelineState, &mut PipelineLog) -> Result<(), String>>;

pub struct PipelineState<'a> {
    pub bytes: &'a [u8],
    pub size: usize,
    pub doc: PartialDocument,
}

#[derive(Default)]
pub struct PartialDocument {
    pub header: Option<String>,
    pub xref_table: Option<XrefTable>,
    pub trailer: Option<HashMap<String, String>>,
    pub objects: Option<HashMap<u32, Object>>,
}

impl<'a> PipelineState<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        PipelineState {
            bytes,
            size: bytes.len(),
            doc: PartialDocument::default(),
        }
    }

    pub fn into_document(self) -> Result<PdfDocument, String> {
        Ok(PdfDocument {
            header: self
                .doc
                .header
                .ok_or_else(|| "Header stage did not run".to_string())?,
            size: self.size,
            xref_table: self
                .doc
                .xref_table
                .ok_or_else(|| "Xref stage did not run".to_string())?,
            trailer: self
                .doc
                .trailer
                .ok_or_else(|| "Trailer missing after xref stage".to_string())?,
            objects: self
                .doc
                .objects
                .ok_or_else(|| "Object stage did not run".to_string())?,
        })
    }
}

impl<'a, 'log> Pipeline<'a, 'log> {
    pub fn new(bytes: &'a [u8], log: &'log mut PipelineLog) -> Self {
        Pipeline {
            stages: Vec::new(),
            state: PipelineState::new(bytes),
            log,
        }
    }

    pub fn add_stage<F>(&mut self, name: &'static str, action: F)
    where
        F: Fn(&mut PipelineState, &mut PipelineLog) -> Result<(), String> + 'static,
    {
        self.stages.push(PipelineStage {
            name,
            action: Box::new(action),
        });
    }

    pub fn run(mut self) -> Result<PdfDocument, String> {
        for stage in self.stages {
            self.log.info(format!("Stage '{}' started", stage.name));
            match (stage.action)(&mut self.state, self.log) {
                Ok(()) => self.log.info(format!("Stage '{}' completed", stage.name)),
                Err(err) => {
                    self.log
                        .error(format!("Stage '{}' failed: {}", stage.name, err));
                    return Err(err);
                }
            }
        }
        self.log.info("All pipeline stages completed");
        self.state.into_document()
    }
}

pub fn finalize_log(pdf_path: &str, log: &PipelineLog) {
    let log_path = derive_log_path(pdf_path);
    match log.write_to(&log_path) {
        Ok(()) => println!("Detailed log written to {}", log_path.display()),
        Err(err) => eprintln!("Failed to write log file: {}", err),
    }
}

fn derive_log_path(pdf_path: &str) -> PathBuf {
    let path = Path::new(pdf_path);
    let mut log_path = path.with_extension("log");
    if log_path.as_os_str().is_empty() {
        log_path = PathBuf::from("pdfreader.log");
    }
    log_path
}
