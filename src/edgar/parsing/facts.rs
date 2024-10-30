use super::types::{FilingFact, Period};
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
    let mut current_period = Period {
        start_date: None,
        end_date: None,
        instant: None,
    };
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
                                // Parse period from context ID
                                if current_context.contains("AsOf") {
                                    current_period.instant = Some(current_context.clone());
                                } else if current_context.contains("From") && current_context.contains("To") {
                                    let parts: Vec<&str> = current_context.split('_').collect();
                                    if parts.len() >= 4 {
                                        current_period.start_date = Some(parts[1].to_string());
                                        current_period.end_date = Some(parts[3].to_string());
                                    }
                                }
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
                    current_period = Period {
                        start_date: None,
                        end_date: None,
                        instant: None,
                    };
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
        // Handle percentage values
        if let Some(unit_str) = unit {
            if unit_str == "Pure" {
                return format!("{:.2}%", num * 100.0);
            }
        }
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
        use quick_xml::events::Event;
        use quick_xml::Reader;

        #[test]
        fn test_period_handling() {
            let xml = r#"
                <ix:nonNumeric name="us-gaap:CashAndCashEquivalents" 
                              contextRef="AsOf_2023-09-30" 
                              unitRef="USD">8069000000</ix:nonNumeric>
                <ix:nonNumeric name="us-gaap:Revenue" 
                              contextRef="From_2023-07-01_To_2023-09-30" 
                              unitRef="USD">23350000000</ix:nonNumeric>"#;
            
            let facts = extract_facts(xml).unwrap();
            assert_eq!(facts.len(), 2);
            
            // Check instant date fact
            let cash = facts.iter()
                .find(|f| f.name.contains("CashAndCashEquivalents"))
                .expect("Should find cash fact");
            assert_eq!(cash.period.instant, Some("AsOf_2023-09-30".to_string()));
            
            // Check period fact
            let revenue = facts.iter()
                .find(|f| f.name.contains("Revenue"))
                .expect("Should find revenue fact");
            assert_eq!(revenue.period.start_date, Some("2023-07-01".to_string()));
            assert_eq!(revenue.period.end_date, Some("2023-09-30".to_string()));
        }

        #[test]
        fn test_format_numeric_values() {
            // Test integer formatting
            assert_eq!(format_fact_value("1234", &None), "1,234");
            assert_eq!(format_fact_value("1000000", &None), "1,000,000");
            
            // Test decimal formatting
            assert_eq!(format_fact_value("1234.56", &None), "1,234.56");
            assert_eq!(format_fact_value("1000000.42", &None), "1,000,000.42");
            
            // Test currency formatting
            assert_eq!(format_fact_value("1234.56", &Some("USD".to_string())), "$1,234.56");
            assert_eq!(format_fact_value("1000000", &Some("USD".to_string())), "$1,000,000");
            
            // Test share formatting
            assert_eq!(format_fact_value("1234", &Some("Shares".to_string())), "1,234 shares");
            assert_eq!(format_fact_value("1000000", &Some("Shares".to_string())), "1,000,000 shares");
            
            // Test percentage formatting
            assert_eq!(format_fact_value("0.1234", &Some("Pure".to_string())), "12.34%");
            assert_eq!(format_fact_value("1.0", &Some("Pure".to_string())), "100.00%");
        }

        #[test]
        fn test_extract_facts() {
            let content = read_test_file("tsla-20230930.htm");
            
            // First pass: examine the XML structure to debug what facts are available
            let mut reader = Reader::from_str(&content);
            let mut buf = Vec::new();
            let mut fact_count = 0;
            let mut found_facts = Vec::new();

            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(ref e)) => {
                        let name = std::str::from_utf8(e.name().as_ref()).unwrap();
                        if name == "ix:nonNumeric" || name == "ix:numeric" {
                            fact_count += 1;
                            for attr in e.attributes() {
                                if let Ok(attr) = attr {
                                    let key = std::str::from_utf8(attr.key.as_ref()).unwrap();
                                    let value = std::str::from_utf8(&attr.value).unwrap();
                                    if key == "name" {
                                        found_facts.push(value.to_string());
                                    }
                                }
                            }
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(e) => panic!("Error parsing XML: {}", e),
                    _ => (),
                }
                buf.clear();
            }

            // Print found facts for debugging
            println!("Found {} facts", fact_count);
            for fact in &found_facts {
                if fact.contains("Share") || fact.contains("share") {
                    println!("Found share-related fact: {}", fact);
                }
            }

            // Now run the actual extraction
            let facts = extract_facts(&content).unwrap();
            
            // Verify we found and parsed the facts correctly
            let cash = facts.iter()
                .find(|f| f.name.contains("CashAndCashEquivalents"))
                .expect("Should find cash fact");
            assert_eq!(cash.unit, Some("USD".to_string()));
            
            // Look for any share-related facts
            let share_facts: Vec<_> = facts.iter()
                .filter(|f| f.name.contains("Share") || f.name.contains("share"))
                .collect();
            
            assert!(!share_facts.is_empty(), "Should find at least one share-related fact");
            
            // Print found share facts for debugging
            for fact in share_facts {
                println!("Share fact: {:?}", fact);
            }
        }
    }
}
