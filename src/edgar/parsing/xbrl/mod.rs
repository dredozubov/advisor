pub mod parser;

// Re-export types from xml parser
pub use self::parser::xml::{DimensionTableRow, FactItem, FactTableRow, XBRLFiling};

// Re-export core functionality
pub use super::types::{FilingDocument, FilingFact, Period};

// Extract facts from XBRL content
pub fn extract_facts(content: &str) -> anyhow::Result<Vec<FilingFact>> {
    let facts = parser::xml::parse_xml_to_facts(content);
    
    // Convert FactItems to FilingFacts
    let filing_facts = facts.into_iter()
        .map(|fact| FilingFact {
            context: fact.context_ref.unwrap_or_default(),
            value: fact.value.clone(),
            unit: fact.unit_ref,
            period: Period {
                start_date: None,
                end_date: None,
                instant: None,
            },
            formatted_value: fact.value,
            name: format!("{}:{}", fact.prefix, fact.name),
        })
        .collect();

    Ok(filing_facts)
}
