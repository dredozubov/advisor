use serde::{Serialize, Deserialize};
use scraper::Html;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct XBRLFiling {
    pub json: Option<Vec<FactItem>>,
    pub facts: Option<Vec<FactTableRow>>,
    pub dimensions: Option<Vec<DimensionTableRow>>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactTableRow {
    pub tag: String,
    pub value: String,
    pub prefix: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub point_in_time: Option<String>,
    pub unit: Option<String>,
    pub num_dim: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DimensionTableRow {
    pub context_ref: String,
    pub axis_prefix: String,
    pub axis_tag: String,
    pub member_prefix: String,
    pub member_tag: String,
}

impl XBRLFiling {
    pub fn new(input_path: String, _email: String, output_types: Vec<&str>) -> Self {
        let content = std::fs::read_to_string(&input_path).expect("Failed to read file");
        
        // Parse XML and extract facts
        let facts = parse_xml_to_facts(&content);
        
        let mut filing = XBRLFiling {
            json: None,
            facts: None,
            dimensions: None
        };

        if output_types.contains(&"json") {
            filing.json = Some(facts.clone());
        }

        if output_types.contains(&"facts") {
            filing.facts = Some(facts_to_table(&facts));
        }

        if output_types.contains(&"dimensions") {
            filing.dimensions = Some(dimensions_to_table(&facts));
        }

        filing
    }
}

pub fn parse_xml_to_facts(content: &str) -> Vec<FactItem> {
    let xml_tree = roxmltree::Document::parse(content).expect("Failed to parse XML");
    let mut facts = Vec::new();
    
    // Process units, periods, and dimensions first
    let mut units = std::collections::HashMap::new();
    let mut periods = std::collections::HashMap::new();
    let mut dimensions = std::collections::HashMap::new();

    // Process units
    for unit_elem in xml_tree.root_element().descendants().filter(|n| n.has_tag_name("unit")) {
        let id = unit_elem.attribute("id").unwrap_or("");
        for measure in unit_elem.descendants().filter(|n| n.has_tag_name("measure")) {
            let name = measure.parent().unwrap().tag_name().name();
            let value = measure.text().unwrap_or("");
            units.entry(id.to_string())
                .or_insert_with(Vec::new)
                .push(Unit {
                    unit_type: name.to_string(),
                    unit_value: value.to_string(),
                });
        }
    }

    // Process contexts (periods and dimensions)
    for context in xml_tree.root_element().descendants().filter(|n| n.has_tag_name("context")) {
        let id = context.attribute("id").unwrap_or("");
        
        // Process periods
        if let Some(period) = context.descendants().find(|n| n.has_tag_name("period")) {
            for child in period.children() {
                if let Some(value) = child.text() {
                    periods.entry(id.to_string())
                        .or_insert_with(Vec::new)
                        .push(Period {
                            period_type: child.tag_name().name().to_string(),
                            period_value: value.to_string(),
                        });
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
                                .or_insert_with(Vec::new)
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

    // Process facts
    let non_fact_elements = ["context", "unit", "xbrl", "schemaRef"];
    for node in xml_tree.root_element().descendants() {
        if let Some(ns) = node.tag_name().namespace() {
            if !non_fact_elements.contains(&node.tag_name().name()) {
                let context_ref = node.attribute("contextRef").map(String::from);
                let unit_ref = node.attribute("unitRef").map(String::from);
                
                let mut fact_dimensions = Vec::new();
                let mut fact_units = Vec::new();
                let mut fact_periods = Vec::new();

                // Look up associated data
                if let Some(ref unit_id) = unit_ref {
                    if let Some(u) = units.get(unit_id) {
                        fact_units = u.clone();
                    }
                }

                if let Some(ref context_id) = context_ref {
                    if let Some(d) = dimensions.get(context_id) {
                        fact_dimensions = d.clone();
                    }
                    if let Some(p) = periods.get(context_id) {
                        fact_periods = p.clone();
                    }
                }

                facts.push(FactItem {
                    id: node.attribute("id").unwrap_or("").to_string(),
                    prefix: node.lookup_prefix(ns).unwrap_or("").to_string(),
                    name: node.tag_name().name().to_string(),
                    value: sanitize_html(node.text().unwrap_or("")),
                    decimals: node.attribute("decimals").unwrap_or("").to_string(),
                    context_ref,
                    unit_ref,
                    dimensions: fact_dimensions,
                    units: fact_units,
                    periods: fact_periods,
                });
            }
        }
    }

    facts
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Unit {
    unit_type: String,
    unit_value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Period {
    period_type: String,
    period_value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Dimension {
    key_ns: String,
    key_value: String,
    member_ns: String,
    member_value: String,
}

fn facts_to_table(facts: &[FactItem]) -> Vec<FactTableRow> {
    facts.iter().map(|fact| {
        FactTableRow {
            tag: fact.name.clone(),
            value: fact.value.clone(),
            prefix: fact.prefix.clone(),
            period_start: None, // Would need context processing
            period_end: None,
            point_in_time: None,
            unit: fact.unit_ref.clone(),
            num_dim: 0,
        }
    }).collect()
}

fn dimensions_to_table(facts: &[FactItem]) -> Vec<DimensionTableRow> {
    let mut dimensions = Vec::new();
    
    for fact in facts {
        if let Some(context) = &fact.context_ref {
            dimensions.push(DimensionTableRow {
                context_ref: context.clone(),
                axis_prefix: fact.prefix.clone(),
                axis_tag: fact.name.clone(),
                member_prefix: "".to_string(),
                member_tag: "".to_string(),
            });
        }
    }

    dimensions
}

fn sanitize_html(input: &str) -> String {
    let fragment = Html::parse_fragment(input);
    fragment.root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}
