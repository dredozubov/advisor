use std::path::PathBuf;
use std::fs;

pub fn get_test_file_path(filename: &str) -> PathBuf {
    PathBuf::from("src/edgar/parsing/tests/data").join(filename)
}

pub fn read_test_file(filename: &str) -> String {
    fs::read_to_string(get_test_file_path(filename))
        .unwrap_or_else(|e| panic!("Failed to read test file {}: {}", filename, e))
}
