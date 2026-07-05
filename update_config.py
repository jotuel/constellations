import sys

def main():
    file_path = "src/settings/config.rs"
    with open(file_path, 'r') as f:
        content = f.read()

    test_code = """
    #[test]
    fn test_config_load_invalid_json() {
        let tmp_dir = tempdir().unwrap();
        let config_path = tmp_dir.path().join("invalid.json");

        std::fs::write(&config_path, "{ invalid json }").unwrap();

        let loaded = Config::load_from(Some(config_path));
        assert_eq!(loaded, Config::default());
    }
"""
    # Find the last closing brace in the file
    last_brace_idx = content.rfind('}')

    if last_brace_idx != -1:
        new_content = content[:last_brace_idx] + test_code + content[last_brace_idx:]
        with open(file_path, 'w') as f:
            f.write(new_content)
        print("Updated config.rs successfully.")
    else:
        print("Could not find the last closing brace in config.rs")

if __name__ == "__main__":
    main()
