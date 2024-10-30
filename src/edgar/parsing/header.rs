use anyhow::Result;
use html_escape::decode_html_entities;
use regex::Regex;
use scraper::{Html, Selector};

pub fn header_parser(raw_html: &str) -> Result<Vec<(String, String)>> {
    let document = Html::parse_document(raw_html);
    let sec_header_selector = Selector::parse("sec-header").unwrap();

    let mut data = Vec::new();

    if let Some(sec_header_element) = document.select(&sec_header_selector).next() {
        let sec_header_html = sec_header_element.inner_html();
        let re = Regex::new(r"<(SEC-HEADER|sec-header)>(.*?)</(SEC-HEADER|sec-header)>")?;

        if let Some(captures) = re.captures(&sec_header_html) {
            if let Some(sec_header) = captures.get(2) {
                let split_header: Vec<&str> = sec_header.as_str().split('\n').collect();

                let mut current_group = String::new();
                for header_item in split_header.iter() {
                    let header_item = header_item.trim();
                    if !header_item.is_empty() {
                        if header_item.starts_with('<') && header_item.ends_with('>') {
                            current_group = header_item.to_string();
                        } else if !header_item.starts_with('\t') && !header_item.contains('<') {
                            if let Some(colon_index) = header_item.find(':') {
                                let key = header_item[..colon_index].trim();
                                let value =
                                    decode_html_entities(&header_item[colon_index + 1..].trim())
                                        .into_owned();
                                data.push((format!("{}:{}", current_group, key), value));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(data)
}
