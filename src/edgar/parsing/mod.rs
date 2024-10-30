pub mod types;
pub mod section;
pub mod extractor;
pub mod cleaner;
pub mod facts;

pub use types::{FilingDocument, FilingSection, FilingFact};
pub use section::SectionParser;
pub use extractor::TextExtractor;
pub use cleaner::TextCleaner;
pub use facts::FactExtractor;

use anyhow::Result;
use std::path::Path;

pub async fn parse_filing(content: &str, output_path: &Path) -> Result<FilingDocument> {
    // Create section parser
    let mut section_parser = SectionParser::new(content);
    
    // Identify and extract sections
    let sections = section_parser.parse()?;
    
    // Extract and clean text from each section
    let mut text_extractor = TextExtractor::new();
    let mut text_cleaner = TextCleaner::new();
    
    let mut processed_sections = Vec::new();
    
    for section in sections {
        let raw_text = text_extractor.extract(&section)?;
        let clean_text = text_cleaner.clean(&raw_text)?;
        
        processed_sections.push(FilingSection {
            section_type: section.section_type,
            title: section.title,
            content: clean_text,
        });
    }
    
    // Extract facts from relevant sections
    let fact_extractor = FactExtractor::new();
    let facts = fact_extractor.extract(&processed_sections)?;
    
    Ok(FilingDocument {
        sections: processed_sections,
        facts,
        path: output_path.to_path_buf(),
    })
}
