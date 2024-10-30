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
    let mut in_fact = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = std::str::from_utf8(e.name().as_ref())?.to_string();

                // Check if this is an XBRL fact tag
                if name.contains(':') {
                    in_fact = true;

                    // Extract context and unit from attributes
                    for attr in e.attributes() {
                        let attr = attr?;
                        match attr.key.as_ref() {
                            b"contextRef" => {
                                current_context = std::str::from_utf8(&attr.value)?.to_string();
                            }
                            b"unitRef" => {
                                current_unit = Some(std::str::from_utf8(&attr.value)?.to_string());
                            }
                            b"period" => {
                                current_period =
                                    Some(std::str::from_utf8(&attr.value)?.to_string());
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
            format!("{}", num as i64)
        } else {
            format!("{:.2}", num)
        };
        
        match unit {
            Some(u) if u.contains("USD") => {
                format!("${}", formatted)
            }
            Some(u) if u.contains("Shares") => {
                format!("{} shares", formatted)
            }
            Some(u) => {
                format!("{} {}", formatted, u)
            }
            None => {
                formatted
            }
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

    #[test]
    fn test_extract_facts() {
        let xml = r#"
            <xbrli:xbrl>
                <us-gaap:Revenue contextRef="FY2020" unitRef="USD">1000000</us-gaap:Revenue>
                <us-gaap:SharesOutstanding contextRef="AsOf2020" unitRef="Shares">50000</us-gaap:SharesOutstanding>
            </xbrli:xbrl>
        "#;

        let facts = extract_facts(xml).unwrap();
        assert_eq!(facts.len(), 2);

        let revenue = &facts[0];
        assert_eq!(revenue.context, "FY2020");
        assert_eq!(revenue.value, "1000000");
        assert_eq!(revenue.formatted_value, "$1,000,000.00");

        let shares = &facts[1];
        assert_eq!(shares.context, "AsOf2020");
        assert_eq!(shares.value, "50000");
        assert_eq!(shares.formatted_value, "50,000 shares");
    }
}
