//! Apple Music song credits parsing.

use malus_model::{CreditCategory, CreditItem, Credits};
use serde_json::Value;

/// Parse Apple JSON response for song credits into normalized `Credits`.
pub fn parse_apple_credits(val: &Value) -> Credits {
    let data = match val.get("data").and_then(|d| d.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => return Credits::empty(),
    };

    let mut categories = Vec::new();

    for item in data {
        let attrs = match item.get("attributes") {
            Some(a) => a,
            None => continue,
        };

        let title = attrs
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string();
        let kind = attrs
            .get("kind")
            .and_then(|k| k.as_str())
            .unwrap_or_default()
            .to_string();

        let mut credit_items = Vec::new();
        if let Some(artists) = item
            .get("relationships")
            .and_then(|r| r.get("credit-artists"))
            .and_then(|c| c.get("data"))
            .and_then(|d| d.as_array())
        {
            for artist in artists {
                let a_attrs = match artist.get("attributes") {
                    Some(a) => a,
                    None => continue,
                };

                let name = match a_attrs.get("name").and_then(|n| n.as_str()) {
                    Some(n) if !n.is_empty() => n.to_string(),
                    _ => continue,
                };

                let roles: Vec<String> = a_attrs
                    .get("roleNames")
                    .and_then(|r| r.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                credit_items.push(CreditItem::new(name, roles));
            }
        }

        if !credit_items.is_empty() || !title.is_empty() {
            categories.push(CreditCategory::new(title, kind, credit_items));
        }
    }

    Credits::new(categories)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_apple_credits() {
        let json = json!({
            "data": [
                {
                    "id": "1-1",
                    "type": "role-categories",
                    "attributes": {
                        "kind": "performer",
                        "title": "PERFORMING ARTISTS"
                    },
                    "relationships": {
                        "credit-artists": {
                            "data": [
                                {
                                    "id": "a-1",
                                    "attributes": {
                                        "name": "Daft Punk",
                                        "roleNames": ["Main Artist", "Vocals"]
                                    }
                                },
                                {
                                    "id": "a-2",
                                    "attributes": {
                                        "name": "Pharrell Williams",
                                        "roleNames": ["Featured Artist", "Vocals"]
                                    }
                                }
                            ]
                        }
                    }
                },
                {
                    "id": "1-2",
                    "type": "role-categories",
                    "attributes": {
                        "kind": "production-and-engineering",
                        "title": "PRODUCTION & ENGINEERING"
                    },
                    "relationships": {
                        "credit-artists": {
                            "data": [
                                {
                                    "id": "a-3",
                                    "attributes": {
                                        "name": "Nile Rodgers",
                                        "roleNames": ["Producer"]
                                    }
                                }
                            ]
                        }
                    }
                }
            ]
        });

        let credits = parse_apple_credits(&json);
        assert_eq!(credits.categories.len(), 2);
        assert_eq!(credits.categories[0].title, "PERFORMING ARTISTS");
        assert_eq!(credits.categories[0].kind, "performer");
        assert_eq!(credits.categories[0].items.len(), 2);
        assert_eq!(credits.categories[0].items[0].name, "Daft Punk");
        assert_eq!(
            credits.categories[0].items[0].roles,
            vec!["Main Artist", "Vocals"]
        );
        assert_eq!(credits.categories[0].items[1].name, "Pharrell Williams");

        assert_eq!(credits.categories[1].title, "PRODUCTION & ENGINEERING");
        assert_eq!(credits.categories[1].items[0].name, "Nile Rodgers");
        assert_eq!(credits.categories[1].items[0].roles, vec!["Producer"]);
    }

    #[test]
    fn test_parse_empty_credits() {
        let json = json!({ "data": [] });
        let credits = parse_apple_credits(&json);
        assert!(credits.is_empty());
    }
}
