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
