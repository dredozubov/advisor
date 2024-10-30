use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;
use super::types::{FilingFact, Period};

// Import core structures from reference implementation
#[derive(Debug, Clone)]
struct Unit {
    unit_type: String,
    unit_value: String,
}

#[derive(Debug, Clone)]
struct Dimension {
    key_ns: String,
    key_value: String,
    member_ns: String,
    member_value: String,
}

pub fn extract_facts(content: &str) -> Result<Vec<FilingFact>> {
    // Normalize whitespace
    let re = Regex::new(r"\s+")?;
    let content = re.replace_all(content, " ").to_string();
    
    // Parse XML document
    let xml_tree = roxmltree::Document::parse(&content)?;
    
    // Initialize storage
    let mut units: HashMap<String, Vec<Unit>> = HashMap::new();
    let mut periods: HashMap<String, Vec<Period>> = HashMap::new();
    let mut dimensions: HashMap<String, Vec<Dimension>> = HashMap::new();

    // Process units
    for unit_elem in xml_tree.root_element().descendants().filter(|n| n.has_tag_name("unit")) {
        let id = unit_elem.attribute("id").unwrap_or("");
        for measure in unit_elem.descendants().filter(|n| n.has_tag_name("measure")) {
            let name = measure.parent().unwrap().tag_name().name();
            let value = measure.text().unwrap_or("");
            units.entry(id.to_string())
                .or_default()
                .push(Unit {
                    unit_type: name.to_string(),
                    unit_value: value.to_string(),
                });
        }
    }

    // Process contexts
    for context in xml_tree.root_element().descendants().filter(|n| n.has_tag_name("context")) {
        let id = context.attribute("id").unwrap_or("");
        
        // Process periods
        if let Some(period) = context.descendants().find(|n| n.has_tag_name("period")) {
            for child in period.children() {
                match child.tag_name().name() {
                    "instant" => {
                        if let Some(value) = child.text() {
                            periods.entry(id.to_string())
                                .or_default()
                                .push(Period {
                                    instant: Some(value.to_string()),
                                    start_date: None,
                                    end_date: None,
                                });
                        }
                    }
                    "startDate" => {
                        if let Some(value) = child.text() {
                            let period = periods.entry(id.to_string()).or_default();
                            period.push(Period {
                                start_date: Some(value.to_string()),
                                end_date: None,
                                instant: None,
                            });
                        }
                    }
                    "endDate" => {
                        if let Some(value) = child.text() {
                            if let Some(last_period) = periods.get_mut(id).and_then(|p| p.last_mut()) {
                                last_period.end_date = Some(value.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Process dimensions
        for segment in context.descendants().filter(|n| n.has_tag_name("segment")) {
            for member in segment.children().filter(|n| n.has_tag_name("explicitMember")) {
                if let Some(dimension) = member.attribute("dimension") {
                    let dim_parts: Vec<&str> = dimension.split(':').collect();
                    if dim_parts.len() == 2 {
                        let value = member.text().unwrap_or("");
                        let value_parts: Vec<&str> = value.split(':').collect();
                        if value_parts.len() == 2 {
                            dimensions.entry(id.to_string())
                                .or_default()
                                .push(Dimension {
                                    key_ns: dim_parts[0].to_string(),
                                    key_value: dim_parts[1].to_string(),
                                    member_ns: value_parts[0].to_string(),
                                    member_value: value_parts[1].to_string(),
                                });
                        }
                    }
                }
            }
        }
    }

    // Extract facts
    let mut facts = Vec::new();
    let non_fact_elements = ["context", "unit", "xbrl", "schemaRef"];

    for fact_elem in xml_tree.root_element().descendants().filter(|n| {
        n.is_element() && 
        !non_fact_elements.contains(&n.tag_name().name()) && 
        n.tag_name().namespace().is_some()
    }) {
        let name = fact_elem.tag_name().name().to_string();
        let namespace = fact_elem.tag_name().namespace().unwrap_or("");
        let prefix = fact_elem.lookup_prefix(namespace).unwrap_or("");
        let context_ref = fact_elem.attribute("contextRef").map(String::from);
        let unit_ref = fact_elem.attribute("unitRef").map(String::from);
        let value = fact_elem.text().unwrap_or("").nfkc().collect::<String>();

        let mut fact_units = Vec::new();
        if let Some(unit_ref_value) = &unit_ref {
            if let Some(unit_list) = units.get(unit_ref_value) {
                fact_units = unit_list.iter()
                    .map(|u| u.unit_value.clone())
                    .collect();
            }
        }

        let period = if let Some(context_id) = &context_ref {
            periods.get(context_id)
                .and_then(|p| p.first())
                .cloned()
                .unwrap_or(Period {
                    start_date: None,
                    end_date: None,
                    instant: None,
                })
        } else {
            Period {
                start_date: None,
                end_date: None,
                instant: None,
            }
        };

        facts.push(FilingFact {
            context: context_ref.unwrap_or_default(),
            value: value.clone(),
            unit: if fact_units.is_empty() { None } else { Some(fact_units.join(" ")) },
            period,
            formatted_value: format_fact_value(&value, &fact_units),
            name: format!("{}:{}", prefix, name),
        });
    }

    Ok(facts)
}

fn format_fact_value(value: &str, units: &[String]) -> String {
    if let Ok(num) = value.parse::<f64>() {
        let formatted = if num.fract() == 0.0 {
            format!("{:.2}", num)
        } else {
            format!("{:.2}", num)
        };

        let parts: Vec<&str> = formatted.split('.').collect();
        let mut result = String::new();
        let chars: Vec<_> = parts[0].chars().collect();
        for (i, c) in chars.iter().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                result.insert(0, ',');
            }
            result.insert(0, *c);
        }

        if parts.len() > 1 {
            result.push('.');
            result.push_str(parts[1]);
        }

        if let Some(unit) = units.first() {
            if unit.contains("USD") {
                format!("${}", result)
            } else if unit.contains("Shares") {
                format!("{} shares", parts[0])
            } else {
                format!("{} {}", result, unit)
            }
        } else {
            result
        }
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edgar::parsing::tests::read_test_file;

    #[test]
    fn test_extract_facts() {
        let content = read_test_file("tsla-20230930.htm");
        let facts = extract_facts(&content).unwrap();
        
        assert!(!facts.is_empty(), "Should extract some facts");
        
        // Test numeric fact formatting
        let numeric_facts: Vec<_> = facts.iter()
            .filter(|f| f.unit.is_some())
            .collect();
        assert!(!numeric_facts.is_empty(), "Should find numeric facts");
        
        // Test currency formatting
        let currency_facts: Vec<_> = numeric_facts.iter()
            .filter(|f| f.unit.as_ref().unwrap().contains("USD"))
            .collect();
        assert!(!currency_facts.is_empty(), "Should find currency facts");
        
        // Test period extraction
        let period_facts: Vec<_> = facts.iter()
            .filter(|f| f.period.instant.is_some() || f.period.start_date.is_some())
            .collect();
        assert!(!period_facts.is_empty(), "Should find facts with periods");
    }
}
