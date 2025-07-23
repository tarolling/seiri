use std::collections::HashMap;

fn read_json_from_file(
    file_path: &str,
) -> Result<HashMap<String, Vec<String>>, Box<dyn std::error::Error>> {
    match std::fs::read_to_string(file_path) {
        Ok(json_data) => Ok(serde_json::from_str(&json_data)?),
        Err(err) => Err(Box::new(err)),
    }
}

pub fn load_language_extensions() -> Result<HashMap<String, Vec<String>>, String> {
    let language_config_file_path = "config/languages.json";
    match read_json_from_file(language_config_file_path) {
        Ok(data) => Ok(data),
        Err(err) => Err(format!("Error reading from config file: {}", err)),
    }
}

mod tests {
    #[test]
    fn test_load_language_extensions() {
        let result = crate::config::load_language_extensions();
        assert!(result.is_ok());

        let map = result.unwrap();
        assert_eq!(map.get("rust"), Some(&vec!["rs".to_string()]));
        assert_eq!(
            map.get("cpp"),
            Some(&vec!["cpp".to_string(), "hpp".to_string()])
        );
        assert_eq!(map.get("go"), None);
    }

    #[test]
    fn test_json_parsing() {
        let string_one = "./path/does/not/exist";
        let string_two = "config/languages.json";
        assert!(crate::config::read_json_from_file(string_one).is_err());
        assert!(crate::config::read_json_from_file(string_two).is_ok());
    }
}
