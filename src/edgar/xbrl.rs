// Based on fast_xbrl_parser: https://github.com/TiesdeKok/fast_xbrl_parser
use regex::Regex;
use scraper::Html;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// Define structs

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InputDetails {
    pub raw_input: String,
    pub cik: String,
    pub accession_number: String,
}

//     For accurate interpretation, we should remember:
// key_ns/key_value represents the axis/dimension
// member_ns/member_value represents the member value
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Dimension {
    pub axis_ns: String,   // was key_ns
    pub axis_name: String, // was key_value
    pub member_ns: String,
    pub member_name: String, // was member_value
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Unit {
    pub unit_type: String,
    pub unit_value: String,
}

impl std::fmt::Display for Unit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} -- {}", self.unit_type, self.unit_value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Period {
    pub period_type: String,
    pub period_value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FactItem {
    pub id: String,
    pub prefix: String,
    pub name: String,
    pub value: String,
    pub decimals: String,
    pub context_ref: Option<String>,
    pub unit_ref: Option<String>,
    pub dimensions: Vec<Dimension>,
    pub units: Vec<Unit>,
    pub periods: Vec<Period>,
}

// Logic for dimensions table

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DimensionTableRow {
    // pub cik: String,
    // pub accession_number: String,
    pub context_ref: String,
    pub axis_prefix: String,
    pub axis_tag: String,
    pub member_prefix: String,
    pub member_tag: String,
}

pub fn dimensions_to_table(facts: Vec<FactItem>) -> Vec<DimensionTableRow> {
    let mut table_rows: Vec<DimensionTableRow> = Vec::new();
    let mut context_ref_tracker = Vec::new();

    // Add rows
    for fact in facts {
        if fact.context_ref.is_some() {
            for dimension in fact.dimensions {
                // This if statement is to prevent duplicate rows
                if !context_ref_tracker.contains(&fact.context_ref.clone().expect("No context ref"))
                {
                    let row = DimensionTableRow {
                        context_ref: fact.context_ref.clone().expect("No context ref"),
                        axis_tag: dimension.axis_name.clone(), // was key_value
                        axis_prefix: dimension.axis_ns.clone(), // was key_ns
                        member_tag: dimension.member_name.clone(), // was member_value
                        member_prefix: dimension.member_ns.clone(),
                    };
                    table_rows.push(row);
                    context_ref_tracker.push(fact.context_ref.clone().expect("No context ref"));
                }
            }
        }
    }

    table_rows
}

// Logic for facts table

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FactTableRow {
    // pub cik: String,
    // pub accession_number: String,
    pub context_ref: Option<String>,
    pub tag: String,
    pub value: String,
    pub prefix: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub point_in_time: Option<String>,
    pub unit: Option<String>,
    pub num_dim: u32,
}

pub fn facts_to_table(facts: Vec<FactItem>) -> Vec<FactTableRow> {
    let mut table_rows: Vec<FactTableRow> = Vec::new();

    //let standard_tags = ["us-gaap", "dei"];

    // Add rows
    for fact in facts {
        let mut row = FactTableRow {
            context_ref: fact.context_ref.clone(),
            tag: fact.name.clone(),
            prefix: fact.prefix.clone(),
            num_dim: fact.dimensions.len() as u32,
            value: fact.value.clone(),
            period_start: None,
            period_end: None,
            point_in_time: None,
            unit: None,
        };

        // Periods are processed into three different columns
        for period in &fact.periods {
            match period.period_type.as_str() {
                "startDate" => row.period_start = Some(period.period_value.clone()),
                "endDate" => row.period_end = Some(period.period_value.clone()),
                "instant" => row.point_in_time = Some(period.period_value.clone()),
                _ => {}
            }
        }

        // The units are converted into a single string
        if !fact.units.is_empty() {
            let tmp = fact
                .units
                .clone()
                .into_iter()
                .map(|e| e.to_string())
                .collect::<Vec<String>>()
                .join(" || ");

            row.unit = Some(tmp.clone());
        }

        table_rows.push(row);
    }

    table_rows
}

pub fn parse_xml_to_facts(raw_xml: String) -> Vec<FactItem> {
    // -- Parse the XML --
    let re = Regex::new(r"\s+").unwrap();
    let raw_xml = re.replace_all(raw_xml.as_str(), " ").to_string();

    let xml_tree = roxmltree::Document::parse(raw_xml.as_str()).expect("Error parsing XML"); // Error handling?

    // -- Get elements out of XML --

    let elem = xml_tree
        .root_element()
        .children()
        .filter(|e| e.node_type() == roxmltree::NodeType::Element);

    // -- Process the context elements --

    let mut units: HashMap<String, Vec<Unit>> = HashMap::new();
    let mut periods: HashMap<String, Vec<Period>> = HashMap::new();
    let mut dimensions: HashMap<String, Vec<Dimension>> = HashMap::new();

    // --- Process the unit elements ---

    let unit_ele = elem.clone().filter(|e| e.tag_name().name() == "unit");
    '_unit_loop: for child in unit_ele.into_iter() {
        let id = child.attribute("id").unwrap_or("");
        let measure_nodes = child
            .descendants()
            .filter(|e| e.tag_name().name() == "measure");

        for m_ele in measure_nodes.into_iter() {
            let name = m_ele.parent().unwrap().tag_name().name();
            let value = m_ele.text().unwrap_or("");
            units.entry(id.to_string()).or_default().push(Unit {
                unit_type: name.to_string(),
                unit_value: value.to_string(),
            });
        }
    }

    // --- Process the context elements ---

    let context_ele = elem.clone().filter(|e| e.tag_name().name() == "context");
    '_context_loop: for child in context_ele.into_iter() {
        let id = child.attribute("id").unwrap_or("");
        log::debug!("ID {}", id);

        let node_desc = child
            .children()
            .filter(|e| e.node_type() == roxmltree::NodeType::Element);

        // loop over descendants and process the different types of elements
        for child_ele in node_desc.into_iter() {
            match child_ele.tag_name().name() {
                "period" => {
                    log::debug!("Found period");

                    let to_keep = ["instant", "startDate", "endDate"];
                    let node_desc_filtered = child_ele
                        .descendants()
                        .filter(|e| to_keep.contains(&e.tag_name().name()));

                    for child_ele_filtered in node_desc_filtered.into_iter() {
                        let value = child_ele_filtered.text().unwrap_or("");
                        let name = child_ele_filtered.tag_name().name();
                        let _namespace = child_ele_filtered.tag_name().namespace().unwrap_or("");

                        periods.entry(id.to_string()).or_default().push(Period {
                            period_type: name.to_string(),
                            period_value: value.to_string(),
                        });

                        log::debug!("Period: {} {}", name, value);
                    }
                }
                "entity" => {
                    log::debug!("Found entity");

                    let to_keep = ["explicitMember"];
                    let node_desc_filtered = child_ele
                        .descendants()
                        .filter(|e| to_keep.contains(&e.tag_name().name()));

                    for child_ele_filtered in node_desc_filtered.into_iter() {
                        let value = child_ele_filtered.text().unwrap_or("");
                        let _name = child_ele_filtered.tag_name().name();
                        let _namespace = child_ele_filtered.tag_name().namespace().unwrap_or("");
                        if child_ele_filtered.has_attribute("dimension") {
                            let dimension_raw = child_ele_filtered.attribute("dimension").unwrap();
                            let dimension_split = dimension_raw.split(":").collect::<Vec<&str>>();
                            let dimension_ns = dimension_split[0];
                            let dimension_value = dimension_split[1];

                            let value_split = value.split(":").collect::<Vec<&str>>();
                            let key_ns = value_split[0];
                            let key_value = value_split[1];

                            dimensions
                                .entry(id.to_string())
                                .or_default()
                                .push(Dimension {
                                    axis_ns: dimension_ns.to_string(), // was key_ns in fast_xbrl_parser
                                    axis_name: dimension_value.to_string(), // was key_value in fast_xbrl_parser
                                    member_ns: key_ns.to_string(),
                                    member_name: key_value.to_string(), // was member_value in fast_xbrl_parser
                                });

                            log::debug!(
                                "Segment: {} {} {} {}",
                                dimension_ns,
                                dimension_value,
                                key_ns,
                                key_value
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // -- Process the fact elements --

    let mut facts: Vec<FactItem> = Vec::new();

    let non_fact_ele = ["context", "unit", "xbrl", "schemaRef"];
    let fact_ele = elem.clone().filter(|e| {
        !&non_fact_ele.contains(&e.tag_name().name()) && e.tag_name().namespace().is_some()
    });

    // loop over fact_ele using enumerate
    '_fact_loop: for child in fact_ele.into_iter() {
        let id = child.attribute("id").unwrap_or(""); // Issue here
        let name: String = child.tag_name().name().to_string();
        let namespace: String = child.tag_name().namespace().unwrap_or("").to_string();
        let prefix = child.lookup_prefix(namespace.as_str()).unwrap_or("");
        let context_ref = &child.attribute("contextRef");
        let unit_ref = &child.attribute("unitRef");
        let decimals = child.attribute("decimals").unwrap_or("");
        let value = child.text().unwrap_or("");

        // Sanitize the value
        let clean_value = sanitize_html(value.to_string().clone());

        let mut fact_dimensions: Vec<Dimension> = Vec::new();
        let mut fact_units: Vec<Unit> = Vec::new();
        let mut fact_periods: Vec<Period> = Vec::new();

        // Look up the units
        if unit_ref.is_some() {
            let unit_ref_value = unit_ref.unwrap().to_string();
            // if unit_ref in units
            if units.contains_key(&unit_ref_value) {
                fact_units = units.get(&unit_ref_value).expect("Unit not found").clone();
            }
        }

        // Look up the dimensions
        if context_ref.is_some() {
            let context_ref_value = context_ref.unwrap().to_string();
            if dimensions.contains_key(&context_ref_value) {
                fact_dimensions = dimensions
                    .get(&context_ref_value)
                    .expect("Dimension not found")
                    .clone();
            }
        }

        // Look up the periods
        if context_ref.is_some() {
            let context_ref_value = context_ref.unwrap().to_string();
            if periods.contains_key(&context_ref_value) {
                fact_periods = periods
                    .get(&context_ref_value)
                    .expect("Period not found")
                    .clone();
            }
        }

        log::debug!(
            "Fact: {} {} {} {} {} {}",
            prefix,
            name,
            value,
            decimals,
            context_ref.unwrap_or("no context"),
            unit_ref.unwrap_or("no unit")
        );

        // Push to vector

        facts.push(FactItem {
            id: id.to_string(),
            prefix: prefix.to_string(),
            name: name.to_string(),
            value: clean_value,
            decimals: decimals.to_string(),
            context_ref: context_ref.map(str::to_string),
            unit_ref: unit_ref.map(str::to_string),
            units: fact_units,
            dimensions: fact_dimensions,
            periods: fact_periods,
        });
    }

    facts
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct XBRLFiling {
    pub json: Option<Vec<FactItem>>,
    pub facts: Option<Vec<FactTableRow>>,
    pub dimensions: Option<Vec<DimensionTableRow>>,
}

impl XBRLFiling {
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        if let Some(facts) = &self.json {
            // Group facts by concept and dimensions signature
            let tables = self.detect_tables(facts);
            let standalone_facts: Vec<&FactItem> = facts
                .iter()
                .filter(|f| {
                    f.dimensions.is_empty()
                        && !tables.values().any(|table_facts| table_facts.contains(f))
                })
                .collect();

            // Generate tables section for facts with dimensions
            if !tables.is_empty() {
                md.push_str("# Data Tables\n\n");
                for (sig, facts) in tables {
                    md.push_str(&self.generate_table(&sig, &facts));
                    md.push_str("\n\n");
                }
            }

            // Generate standalone facts section in compact format
            if !standalone_facts.is_empty() {
                md.push_str("# Facts\n\n");
                for fact in standalone_facts {
                    md.push_str(&self.format_compact_fact(fact));
                    md.push('\n');
                }
            }
        }
        md
    }

    fn detect_tables<'a>(&self, facts: &'a [FactItem]) -> HashMap<String, Vec<&'a FactItem>> {
        let mut tables: HashMap<String, Vec<&FactItem>> = HashMap::new();

        // Group facts by name and dimension structure
        for fact in facts {
            // Create signature combining concept name and dimension axes
            let mut dim_axes: Vec<String> = fact
                .dimensions
                .iter()
                .map(|d| format!("{}:{}", d.axis_ns, d.axis_name))
                .collect();
            dim_axes.sort();
            let signature = if dim_axes.is_empty() {
                fact.name.clone()
            } else {
                format!("{}_{}", fact.name, dim_axes.join("_"))
            };

            // Only create tables for facts that appear multiple times
            tables.entry(signature).or_default().push(fact);
        }

        // Remove single-fact "tables"
        tables.retain(|_, facts| facts.len() > 1);
        tables
    }

    fn generate_table(&self, _signature: &str, facts: &[&FactItem]) -> String {
        let mut md = String::new();

        if facts.is_empty() {
            return md;
        }

        let sample_fact = facts[0];

        // Table title
        md.push_str(&format!("## {}\n\n", sample_fact.name));

        // Collect all unique dimensions and periods
        let mut all_periods = HashSet::new();
        let mut dim_members: HashMap<String, HashSet<String>> = HashMap::new();

        for fact in facts {
            // Collect periods
            for period in &fact.periods {
                all_periods.insert(format!("{}: {}", period.period_type, period.period_value));
            }

            // Collect dimension members
            for dim in &fact.dimensions {
                dim_members
                    .entry(format!("{}:{}", dim.axis_ns, dim.axis_name))
                    .or_default()
                    .insert(format!("{}:{}", dim.member_ns, dim.member_name));
            }
        }

        // Generate table headers
        let mut headers = vec!["Period".to_string()];
        let dimensions: Vec<String> = dim_members.keys().cloned().collect();
        headers.extend(dimensions.iter().cloned());
        headers.push("Value".to_string());

        // Header row
        md.push('|');
        for header in &headers {
            md.push_str(&format!(" {} |", header));
        }
        md.push_str("\n|");

        // Separator row
        for _ in &headers {
            md.push_str(" --- |");
        }
        md.push('\n');

        // Data rows
        let periods: Vec<String> = all_periods.into_iter().collect();
        for period in periods {
            for fact in facts {
                if fact
                    .periods
                    .iter()
                    .any(|p| format!("{}: {}", p.period_type, p.period_value) == period)
                {
                    md.push_str(&format!("| {} ", period));

                    // Add dimension values
                    for dim_axis in &dimensions {
                        let member = fact
                            .dimensions
                            .iter()
                            .find(|d| format!("{}:{}", d.axis_ns, d.axis_name) == *dim_axis)
                            .map(|d| format!("{}:{}", d.member_ns, d.member_name))
                            .unwrap_or_else(|| "-".to_string());
                        md.push_str(&format!("| {} ", member));
                    }

                    // Add value
                    md.push_str(&format!("| {} |\n", fact.value));
                }
            }
        }

        md
    }

    fn format_compact_fact(&self, fact: &FactItem) -> String {
        let mut parts = vec![format!("{}:{}", fact.prefix, fact.name)];

        // Add value
        parts.push(fact.value.clone());

        // Add period(s) with "/" separator for ranges
        if !fact.periods.is_empty() {
            let mut start_date = None;
            let mut end_date = None;

            for period in &fact.periods {
                match period.period_type.as_str() {
                    "startDate" => start_date = Some(period.period_value.clone()),
                    "endDate" => end_date = Some(period.period_value.clone()),
                    "instant" => start_date = Some(period.period_value.clone()),
                    _ => {}
                }
            }

            let period_str = match (start_date, end_date) {
                (Some(start), Some(end)) if start == end => format!("({})", start),
                (Some(start), Some(end)) => format!("({}/{})", start, end),
                (Some(date), None) | (None, Some(date)) => format!("({})", date),
                (None, None) => String::new(),
            };

            if !period_str.is_empty() {
                parts.push(period_str);
            }
        }

        parts.join(" ")
    }
}

fn sanitize_html(input: String) -> String {
    let mut output = input.clone();

    // Remove non ascii characters and replace them with a space
    output = output.replace(|c: char| !c.is_ascii(), " ");

    // Remove HTML
    if output.contains("<") {
        // Remove HTML tags
        let fragment = Html::parse_fragment(output.as_str());
        let root = fragment.root_element();
        output = root.text().collect::<Vec<_>>().join(" ");
    }

    // Remove duplicate white spaces

    let re = Regex::new(r"\s+").unwrap();
    re.replace_all(output.as_str(), " ").to_string()
}
