use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealthStatus {
    pub name: String,
    pub version: String,
}

pub fn current() -> HealthStatus {
    HealthStatus {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_reports_the_package_name_and_version() {
        assert_eq!(
            current(),
            HealthStatus {
                name: "anki-mining".to_string(),
                version: "0.1.0".to_string(),
            }
        );
    }

    #[test]
    fn serializes_with_the_field_names_expected_by_the_frontend() {
        let json = serde_json::to_value(current()).expect("health status must serialize");

        assert_eq!(
            json,
            serde_json::json!({ "name": "anki-mining", "version": "0.1.0" })
        );
    }
}
