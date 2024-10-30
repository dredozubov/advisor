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

    for node in xml_tree.root_element().descendants() {
        if let Some(ns) = node.tag_name().namespace() {
            if !["context", "unit", "xbrl", "schemaRef"].contains(&node.tag_name().name()) {
                facts.push(FactItem {
                    id: node.attribute("id").unwrap_or("").to_string(),
                    prefix: node.lookup_prefix(ns).unwrap_or("").to_string(),
                    name: node.tag_name().name().to_string(),
                    value: sanitize_html(node.text().unwrap_or("")),
                    decimals: node.attribute("decimals").unwrap_or("").to_string(),
                    context_ref: node.attribute("contextRef").map(String::from),
                    unit_ref: node.attribute("unitRef").map(String::from),
                });
            }
        }
    }

    facts
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
