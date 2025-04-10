use std::time::{SystemTime, UNIX_EPOCH};

/// Get the current unix timestamp
pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Validate a provided name value
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.len() == 0 {
        return Err("name string must not be empty".into());
    }
    for c in name.chars() {
        if matches!(c, '+' | '/' | '#') {
            return Err(format!(
                "name string {name} cannot contain '+', '/' or '#' characters"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valiate_name_valid_strings() {
        assert!(validate_name("hello").is_ok());
        assert!(validate_name("hello123").is_ok());
        assert!(validate_name("hello_world").is_ok());
    }

    #[test]
    fn test_validate_name_invalid_strings() {
        assert!(validate_name("").is_err());
        assert!(validate_name("hello+world").is_err());
        assert!(validate_name("hello/world").is_err());
        assert!(validate_name("hello#world").is_err());
        assert!(validate_name("hello+/world").is_err());
        assert!(validate_name("hello+/#world").is_err());
    }
}
