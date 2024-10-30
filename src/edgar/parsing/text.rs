use anyhow::Result;
use unicode_normalization::UnicodeNormalization;
use regex::Regex;
use html_escape::decode_html_entities;
use std::collections::HashMap;

pub fn process_section_text(content: &str) -> Result<String> {
    let mut text = content.to_string();

    // Step 1: Convert HTML entities
    text = decode_html_entities(&text).into_owned();

    // Step 2: Remove scripts and styles
    let script_re = Regex::new(r"(?is)<script.*?</script>")?;
    let style_re = Regex::new(r"(?is)<style.*?</style>")?;
    text = script_re.replace_all(&text, "").to_string();
    text = style_re.replace_all(&text, "").to_string();

    // Step 3: Convert tables to markdown
    text = convert_tables_to_markdown(&text)?;

    // Step 4: Preserve line breaks and lists
    let br_re = Regex::new(r"<br\s*/?>|</p>|</div>")?;
    text = br_re.replace_all(&text, "\n").to_string();
    
    // Convert ordered lists
    let ol_re = Regex::new(r"(?s)<ol.*?>(.*?)</ol>")?;
    text = ol_re.replace_all(&text, |caps: &regex::Captures| {
        process_ordered_list(&caps[1])
    }).to_string();

    // Convert unordered lists
    let ul_re = Regex::new(r"(?s)<ul.*?>(.*?)</ul>")?;
    text = ul_re.replace_all(&text, |caps: &regex::Captures| {
        process_unordered_list(&caps[1])
    }).to_string();

    // Step 5: Strip remaining HTML tags
    let tag_re = Regex::new(r"<[^>]+>")?;
    text = tag_re.replace_all(&text, "").to_string();

    // Step 6: Clean up whitespace
    let whitespace_re = Regex::new(r"\s+")?;
    text = whitespace_re.replace_all(&text, " ").to_string();
    text = text.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    // Step 7: Normalize Unicode
    text = text.nfkc().collect::<String>();

    Ok(text)
}

fn convert_tables_to_markdown(html: &str) -> Result<String> {
    let table_re = Regex::new(r"(?s)<table.*?>(.*?)</table>")?;
    let tr_re = Regex::new(r"(?s)<tr.*?>(.*?)</tr>")?;
    let td_re = Regex::new(r"(?s)<t[dh].*?>(.*?)</t[dh]>")?;

    let mut result = html.to_string();
    result = table_re.replace_all(&result, |caps: &regex::Captures| {
        let table_content = &caps[1];
        let mut markdown_table = String::new();
        
        // Process rows
        for (i, row) in tr_re.captures_iter(table_content).enumerate() {
            let cells: Vec<String> = td_re.captures_iter(&row[1])
                .map(|cell| cell[1].trim().replace('\n', " "))
                .collect();

            // Add cells with pipe separators
            markdown_table.push_str(&format!("| {} |\n", cells.join(" | ")));

            // Add header separator after first row
            if i == 0 {
                markdown_table.push_str(&format!("| {} |\n", cells.iter()
                    .map(|_| "---")
                    .collect::<Vec<_>>()
                    .join(" | ")));
            }
        }
        markdown_table
    }).to_string();

    Ok(result)
}

fn process_ordered_list(list_content: &str) -> String {
    let li_re = Regex::new(r"(?s)<li.*?>(.*?)</li>").unwrap();
    li_re.captures_iter(list_content)
        .enumerate()
        .map(|(i, cap)| format!("{}. {}", i + 1, cap[1].trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn process_unordered_list(list_content: &str) -> String {
    let li_re = Regex::new(r"(?s)<li.*?>(.*?)</li>").unwrap();
    li_re.captures_iter(list_content)
        .map(|cap| format!("* {}", cap[1].trim()))
        .collect::<Vec<_>>()
        .join("\n")
}
