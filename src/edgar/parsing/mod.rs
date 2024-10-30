pub mod types;
pub mod document;
pub mod section;
pub mod text;
pub mod tests;
pub mod xbrl;

pub use types::{FilingDocument, FilingSection, FilingFact, SectionType};
pub use document::{parse_documents, header_parser};
pub use xbrl::{extract_facts, parse_filing};

use anyhow::Result;
use quick_xml::Reader;
use std::path::Path;

pub struct XBRLParser {
    reader: Reader<std::io::BufReader<std::fs::File>>,
    output_path: std::path::PathBuf,
}

impl XBRLParser {
    pub fn new(path: &Path, output_path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let reader = Reader::from_reader(std::io::BufReader::new(file));
        Ok(Self { 
            reader,
            output_path: output_path.to_path_buf(),
        })
    }

    pub fn parse(&mut self) -> Result<FilingDocument> {
        // 1. First pass: Identify sections
        let sections = self.identify_sections()?;
        
        // 2. Second pass: Extract text and facts
        let (processed_sections, facts) = self.extract_content(&sections)?;

        Ok(FilingDocument {
            sections: processed_sections,
            facts,
            path: self.output_path.clone(),
        })
    }

    fn identify_sections(&mut self) -> Result<Vec<FilingSection>> {
        // Implementation will be in section.rs
        section::identify_sections(&mut self.reader)
    }

    fn extract_content(
        &mut self,
        sections: &[FilingSection],
    ) -> Result<(Vec<FilingSection>, Vec<FilingFact>)> {
        let mut processed_sections = Vec::new();
        let mut facts = Vec::new();

        for section in sections {
            let processed_text = text::process_section_text(&section.content)?;
            let section_facts = xbrl::extract_facts(&section.content)?;
            
            processed_sections.push(FilingSection {
                section_type: section.section_type.clone(),
                title: section.title.clone(),
                content: processed_text,
            });

            facts.extend(section_facts);
        }

        Ok((processed_sections, facts))
    }
}

// Public interface
pub fn parse_filing(path: &Path) -> Result<FilingDocument> {
    let mut parser = XBRLParser::new(path, path.parent().unwrap_or_else(|| Path::new("")))?;
    parser.parse()
}
