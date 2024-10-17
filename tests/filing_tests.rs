use claude_api_interaction::edgar::filing::{decode_uuencoded, extract_complete_submission_filing};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_decode_uuencoded() {
    let uuencoded_content = r#"begin 644 test.txt
M2&5R2&5R(&ES(&$@=&5S="!O9B!55T5N8V]D:6YG+@H*5&AI<R!I<R!A;F]T
M:&5R(&QI;F4@;V8@=&5X="X*"D5N9"!O9B!T:&4@=&5S="!F:6QE+@H`
end
"#;
    let decoded = decode_uuencoded(uuencoded_content).unwrap();
    let decoded_text = String::from_utf8(decoded).unwrap();
    assert_eq!(decoded_text, "Here is a test of UUEncoding.\n\nThis is another line of text.\n\nEnd of the test file.\n");
}

#[test]
fn test_extract_complete_submission_filing_with_uuencoded_content() {
    let temp_dir = tempdir().unwrap();
    let input_file = temp_dir.path().join("input.txt");
    let uuencoded_content = r#"<DOCUMENT>
<TYPE>EX-101.INS
<FILENAME>uuencoded_content.txt
<DESCRIPTION>XBRL INSTANCE DOCUMENT
<TEXT>
begin 644 test.txt
M2&5R2&5R(&ES(&$@=&5S="!O9B!55T5N8V]D:6YG+@H*5&AI<R!I<R!A;F]T
M:&5R(&QI;F4@;V8@=&5X="X*"D5N9"!O9B!T:&4@=&5S="!F:6QE+@H`
end
</TEXT>
</DOCUMENT>"#;
    fs::write(&input_file, uuencoded_content).unwrap();

    let output_dir = temp_dir.path().join("output");
    let result = extract_complete_submission_filing(input_file.to_str().unwrap(), Some(&output_dir)).unwrap();

    assert_eq!(result.len(), 1);
    let extracted_file = output_dir.join("0001-(EX-101.INS) XBRL_INSTANCE_DOCUMENT uuencoded_content.txt");
    assert!(extracted_file.exists());

    let content = fs::read_to_string(extracted_file).unwrap();
    assert_eq!(content, "Here is a test of UUEncoding.\n\nThis is another line of text.\n\nEnd of the test file.\n");
}
