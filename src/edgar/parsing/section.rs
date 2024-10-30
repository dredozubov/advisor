use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use std::collections::HashMap;

use super::types::{FilingSection, SectionType};

pub fn identify_sections(reader: &mut Reader<std::io::BufReader<std::fs::File>>) -> Result<Vec<FilingSection>> {
    let mut sections = Vec::new();
    let mut buf = Vec::new();
    let mut current_section = String::new();
    let mut in_section = false;
    
    // Common section identifiers
    let section_patterns = HashMap::from([
        ("item 7", SectionType::ManagementDiscussion),
        ("management's discussion", SectionType::ManagementDiscussion),
        ("financial statements", SectionType::FinancialStatements),
        ("notes to", SectionType::Notes),
        ("risk factors", SectionType::RiskFactors),
        ("business", SectionType::BusinessDescription),
    ]);

    let section_regex = Regex::new(r"(?i)^\s*(item\s+\d+[.:])?\s*(.+?)(?:\s*\(continued\))?\s*$")?;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = std::str::from_utf8(e.name().as_ref())?;
                if name == "div" || name == "section" {
                    for attr in e.attributes() {
                        let attr = attr?;
                        if attr.key.as_ref() == b"class" || attr.key.as_ref() == b"id" {
                            let value = std::str::from_utf8(&attr.value)?;
                            if value.contains("section") || value.contains("item") {
                                in_section = true;
                                break;
                            }
                        }
                    }
                }
            }
            Ok(Event::Text(e)) => {
                if in_section {
                    let text = e.unescape()?.to_string();
                    current_section.push_str(&text);
                }
            }
            Ok(Event::End(ref e)) => {
                let name = std::str::from_utf8(e.name().as_ref())?;
                if (name == "div" || name == "section") && in_section {
                    if !current_section.trim().is_empty() {
                        // Try to identify section type
                        let section_type = identify_section_type(&current_section, &section_patterns);
                        let title = extract_section_title(&current_section, &section_regex)?;
                        
                        sections.push(FilingSection {
                            section_type,
                            title,
                            content: current_section.clone(),
                        });
                    }
                    current_section.clear();
                    in_section = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("Error parsing XML: {}", e)),
            _ => (),
        }
        buf.clear();
    }

    Ok(sections)
}

fn identify_section_type(content: &str, patterns: &HashMap<&str, SectionType>) -> SectionType {
    let content_lower = content.to_lowercase();
    for (pattern, section_type) in patterns {
        if content_lower.contains(pattern) {
            return section_type.clone();
        }
    }
    SectionType::Other("Unknown".to_string())
}

fn extract_section_title(content: &str, regex: &Regex) -> Result<String> {
    let lines: Vec<&str> = content.lines().take(5).collect();
    for line in lines {
        if let Some(captures) = regex.captures(line) {
            if let Some(title) = captures.get(2) {
                return Ok(title.as_str().trim().to_string());
            }
        }
    }
    Ok("Untitled Section".to_string())
}
