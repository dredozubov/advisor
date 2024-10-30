use super::types::FilingFact;
use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;

pub fn extract_facts(content: &str) -> Result<Vec<FilingFact>> {
    let mut facts = Vec::new();
    let mut reader = Reader::from_str(content);
    let mut buf = Vec::new();

    let mut current_context = String::new();
    let mut current_value = String::new();
    let mut current_unit = None;
    let mut current_period = None;
    let mut current_name = String::new();
    let mut in_fact = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = std::str::from_utf8(e.name().as_ref())?.to_string();

                // Check if this is an iXBRL fact tag
                if name == "ix:nonNumeric" || name == "ix:numeric" {
                    in_fact = true;

                    // Extract attributes
                    for attr in e.attributes() {
                        let attr = attr?;
                        match attr.key.as_ref() {
                            b"contextRef" => {
                                current_context = std::str::from_utf8(&attr.value)?.to_string();
                            }
                            b"unitRef" => {
                                current_unit = Some(std::str::from_utf8(&attr.value)?.to_string());
                            }
                            b"name" => {
                                current_name = std::str::from_utf8(&attr.value)?.to_string();
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::Text(e)) if in_fact => {
                current_value = e.unescape()?.into_owned();
            }
            Ok(Event::End(ref e)) => {
                let name = std::str::from_utf8(e.name().as_ref())?.to_string();
                if name.contains(':') && in_fact {
                    // Format the value based on type
                    let formatted_value = format_fact_value(&current_value, &current_unit);

                    facts.push(FilingFact {
                        context: current_context.clone(),
                        value: current_value.clone(),
                        unit: current_unit.clone(),
                        period: current_period.clone(),
                        formatted_value,
                        name: current_name.clone(),
                    });

                    // Reset state
                    current_value.clear();
                    current_unit = None;
                    current_period = None;
                    in_fact = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("Error parsing XML: {}", e)),
            _ => (),
        }
        buf.clear();
    }

    Ok(facts)
}

fn format_fact_value(value: &str, unit: &Option<String>) -> String {
    // Try to parse as number first
    if let Ok(num) = value.parse::<f64>() {
        let formatted = if num.fract() == 0.0 {
            // Format integer with .00 decimal places
            let formatted = format!("{:.2}", num);
            // Split into integer and decimal parts
            let parts: Vec<&str> = formatted.split('.').collect();
            let int_part = parts[0];

            // Add thousands separators to integer part
            let mut result = String::new();
            let chars: Vec<_> = int_part.chars().collect();
            for (i, c) in chars.iter().rev().enumerate() {
                if i > 0 && i % 3 == 0 {
                    result.insert(0, ',');
                }
                result.insert(0, *c);
            }

            result
        } else {
            let formatted = format!("{:.2}", num);
            // Split into integer and decimal parts
            let parts: Vec<&str> = formatted.split('.').collect();
            let int_part = parts[0];
            let dec_part = parts.get(1).unwrap_or(&"00");

            // Add thousands separators to integer part
            let mut result = String::new();
            let chars: Vec<_> = int_part.chars().collect();
            for (i, c) in chars.iter().rev().enumerate() {
                if i > 0 && i % 3 == 0 {
                    result.insert(0, ',');
                }
                result.insert(0, *c);
            }

            // Add decimal part back
            format!("{}.{}", result, dec_part)
        };

        match unit {
            Some(u) if u.contains("USD") => {
                format!("${}", formatted)
            }
            Some(u) if u.contains("Shares") => {
                // For shares, format without decimal places
                let parts: Vec<&str> = formatted.split('.').collect();
                format!("{} shares", parts[0])
            }
            Some(u) => {
                format!("{} {}", formatted, u)
            }
            None => formatted,
        }
    } else {
        // Return as-is if not numeric
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_fact_value() {
        assert_eq!(
            format_fact_value("1234.56", &Some("USD".to_string())),
            "$1,234.56"
        );
        assert_eq!(
            format_fact_value("1000000", &Some("Shares".to_string())),
            "1,000,000 shares"
        );
        assert_eq!(format_fact_value("1234", &None), "1,234");
        assert_eq!(format_fact_value("text", &None), "text");
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::edgar::parsing::tests::read_test_file;

        #[test]
        fn test_extract_facts() {
            let content = read_test_file("tsla-20230930.htm");
            let facts = extract_facts(&content).unwrap();
            
            println!("Found {} facts", facts.len());
            for fact in &facts {
                println!("Fact: name={}, context={}, unit={:?}, value={}, formatted={}",
                    fact.name, fact.context, fact.unit, fact.value, fact.formatted_value);
            }
            
            // First just check we found some facts
            assert!(!facts.is_empty(), "No facts were extracted from the file");
            
            // Then look for specific ones
            if let Some(cash) = facts.iter()
                .find(|f| f.name.contains("CashAndCashEquivalents")) {
                println!("Found cash fact: {:?}", cash);
            } else {
                println!("No cash fact found");
            }

            if let Some(shares) = facts.iter()
                .find(|f| f.name.contains("CommonStock")) {
                println!("Found shares fact: {:?}", shares);
            } else {
                println!("No shares fact found");
            }
        }
    }
}
