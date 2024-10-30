pub mod types;
pub mod document;

pub use types::{FilingDocument, FilingSection, FilingFact};
pub use document::{parse_documents, header_parser};

use anyhow::Result;
use std::path::Path;

pub async fn parse_filing(content: &str, output_path: &Path) -> Result<FilingDocument> {
    let mut sections = Vec::new();
    let mut facts = Vec::new();
    
    // Basic section extraction using regex
    let section_re = Regex::new(r"<SECTION>(.*?)</SECTION>")?;
    for cap in section_re.captures_iter(content) {
        if let Some(section_content) = cap.get(1) {
            sections.push(FilingSection {
                section_type: SectionType::Other("Unknown".to_string()),
                title: "Untitled Section".to_string(),
                content: section_content.as_str().to_string(),
            });
        }
    }

    Ok(FilingDocument {
        sections,
        facts,
        path: output_path.to_path_buf(),
    })
}
mod section;
mod text;
mod facts;
mod types;

use anyhow::Result;
use quick_xml::Reader;
use std::path::Path;

pub use types::{FilingDocument, FilingFact, FilingSection, SectionType};

pub struct XBRLParser {
    reader: Reader<std::io::BufReader<std::fs::File>>,
}

impl XBRLParser {
    pub fn new(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let reader = Reader::from_reader(std::io::BufReader::new(file));
        Ok(Self { reader })
    }

    pub fn parse(&mut self) -> Result<FilingDocument> {
        // 1. First pass: Identify sections
        let sections = self.identify_sections()?;
        
        // 2. Second pass: Extract text and facts
        let (processed_sections, facts) = self.extract_content(&sections)?;

        Ok(FilingDocument {
            sections: processed_sections,
            facts,
            path: self.reader.path().to_owned(),
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
            let section_facts = facts::extract_facts(&section.content)?;
            
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
    let mut parser = XBRLParser::new(path)?;
    parser.parse()
}
