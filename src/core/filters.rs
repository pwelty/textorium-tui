use crate::core::posts::Post;

/// Supported filter operators for property filters.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOp {
    Contains,
    Equals,
    IsTrue,
    IsFalse,
}

impl FilterOp {
    /// Parse a string like "contains", "equals", "is_true", "is_false".
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "contains" => Some(FilterOp::Contains),
            "equals" | "eq" => Some(FilterOp::Equals),
            "is_true" | "istrue" | "true" => Some(FilterOp::IsTrue),
            "is_false" | "isfalse" | "false" => Some(FilterOp::IsFalse),
            _ => None,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            FilterOp::Contains => "contains",
            FilterOp::Equals => "equals",
            FilterOp::IsTrue => "is_true",
            FilterOp::IsFalse => "is_false",
        }
    }
}

/// A single property filter: field op [value].
#[derive(Debug, Clone)]
pub struct PropertyFilter {
    pub field: String,
    pub op: FilterOp,
    /// Value is only meaningful for `contains` and `equals`.
    pub value: String,
}

impl PropertyFilter {
    /// Parse a filter string in the form `field:op:value` or `field:op` (for bool ops).
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() < 2 {
            return None;
        }
        let field = parts[0].trim().to_string();
        let op = FilterOp::parse(parts[1].trim())?;
        let value = parts
            .get(2)
            .map(|v| v.trim().to_string())
            .unwrap_or_default();

        // Validate: contains and equals need a value
        if matches!(op, FilterOp::Contains | FilterOp::Equals) && value.is_empty() {
            return None;
        }

        Some(PropertyFilter { field, op, value })
    }

    /// Human-readable description of this filter for display in the status bar.
    pub fn display(&self) -> String {
        match self.op {
            FilterOp::Contains => format!("{}:contains:{}", self.field, self.value),
            FilterOp::Equals => format!("{}:equals:{}", self.field, self.value),
            FilterOp::IsTrue => format!("{}:is_true", self.field),
            FilterOp::IsFalse => format!("{}:is_false", self.field),
        }
    }

    /// Test whether a post matches this filter.
    pub fn matches(&self, post: &Post) -> bool {
        let fm_value = post.frontmatter.get(&self.field);

        match &self.op {
            FilterOp::IsTrue => fm_value.and_then(|v| v.as_bool()).unwrap_or(false),
            FilterOp::IsFalse => {
                // is_false: field is present and false, OR field absent (treat as false)
                match fm_value {
                    Some(v) => v.as_bool() == Some(false),
                    None => true,
                }
            }
            FilterOp::Equals => {
                let target = self.value.to_lowercase();
                match fm_value {
                    Some(serde_json::Value::String(s)) => s.to_lowercase() == target,
                    Some(serde_json::Value::Bool(b)) => b.to_string() == target,
                    Some(serde_json::Value::Number(n)) => n.to_string() == target,
                    Some(serde_json::Value::Array(arr)) => arr.iter().any(|v| {
                        v.as_str()
                            .map(|s| s.to_lowercase() == target)
                            .unwrap_or(false)
                    }),
                    _ => false,
                }
            }
            FilterOp::Contains => {
                let target = self.value.to_lowercase();
                match fm_value {
                    Some(serde_json::Value::String(s)) => s.to_lowercase().contains(&target),
                    Some(serde_json::Value::Array(arr)) => arr.iter().any(|v| {
                        v.as_str()
                            .map(|s| s.to_lowercase().contains(&target))
                            .unwrap_or(false)
                    }),
                    _ => false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_post_with_fm(fm: HashMap<String, serde_json::Value>) -> Post {
        let mut post = Post {
            path: PathBuf::from("/tmp/test.md"),
            title: fm
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Test")
                .to_string(),
            date: None,
            draft: fm.get("draft").and_then(|v| v.as_bool()).unwrap_or(false),
            content_type: String::new(),
            categories: Vec::new(),
            tags: Vec::new(),
            content: String::new(),
            frontmatter: fm.clone(),
            raw_frontmatter: String::new(),
            original_frontmatter: fm,
            original_content: String::new(),
            format: crate::core::posts::FrontmatterFormat::default(),
        };
        post.sync_fields_from_frontmatter();
        post
    }

    fn fm_with(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn test_parse_filter_contains() {
        let f = PropertyFilter::parse("tags:contains:rust").unwrap();
        assert_eq!(f.field, "tags");
        assert!(matches!(f.op, FilterOp::Contains));
        assert_eq!(f.value, "rust");
    }

    #[test]
    fn test_parse_filter_equals() {
        let f = PropertyFilter::parse("content_type:equals:tutorial").unwrap();
        assert_eq!(f.field, "content_type");
        assert!(matches!(f.op, FilterOp::Equals));
        assert_eq!(f.value, "tutorial");
    }

    #[test]
    fn test_parse_filter_is_true() {
        let f = PropertyFilter::parse("draft:is_true").unwrap();
        assert_eq!(f.field, "draft");
        assert!(matches!(f.op, FilterOp::IsTrue));
    }

    #[test]
    fn test_parse_filter_is_false() {
        let f = PropertyFilter::parse("draft:is_false").unwrap();
        assert!(matches!(f.op, FilterOp::IsFalse));
    }

    #[test]
    fn test_parse_filter_missing_value_fails() {
        assert!(PropertyFilter::parse("title:contains").is_none());
        assert!(PropertyFilter::parse("title:equals").is_none());
    }

    #[test]
    fn test_parse_filter_unknown_op_fails() {
        assert!(PropertyFilter::parse("title:startswith:foo").is_none());
    }

    #[test]
    fn test_matches_contains_string() {
        let post = make_post_with_fm(fm_with(&[(
            "title",
            serde_json::Value::String("Rust and TUI".to_string()),
        )]));
        let f = PropertyFilter::parse("title:contains:rust").unwrap();
        assert!(f.matches(&post));
    }

    #[test]
    fn test_matches_contains_array() {
        let post = make_post_with_fm(fm_with(&[(
            "tags",
            serde_json::Value::Array(vec![
                serde_json::Value::String("rust".to_string()),
                serde_json::Value::String("cli".to_string()),
            ]),
        )]));
        let f = PropertyFilter::parse("tags:contains:rust").unwrap();
        assert!(f.matches(&post));

        let f2 = PropertyFilter::parse("tags:contains:python").unwrap();
        assert!(!f2.matches(&post));
    }

    #[test]
    fn test_matches_equals_case_insensitive() {
        let post = make_post_with_fm(fm_with(&[(
            "content_type",
            serde_json::Value::String("Tutorial".to_string()),
        )]));
        let f = PropertyFilter::parse("content_type:equals:tutorial").unwrap();
        assert!(f.matches(&post));
    }

    #[test]
    fn test_matches_is_true() {
        let post = make_post_with_fm(fm_with(&[("draft", serde_json::Value::Bool(true))]));
        let f = PropertyFilter::parse("draft:is_true").unwrap();
        assert!(f.matches(&post));

        let post2 = make_post_with_fm(fm_with(&[("draft", serde_json::Value::Bool(false))]));
        assert!(!f.matches(&post2));
    }

    #[test]
    fn test_matches_is_false_absent_field() {
        // Field absent → treated as false
        let post = make_post_with_fm(HashMap::new());
        let f = PropertyFilter::parse("draft:is_false").unwrap();
        assert!(f.matches(&post));
    }

    #[test]
    fn test_display() {
        let f = PropertyFilter::parse("tags:contains:rust").unwrap();
        assert_eq!(f.display(), "tags:contains:rust");

        let f2 = PropertyFilter::parse("draft:is_true").unwrap();
        assert_eq!(f2.display(), "draft:is_true");
    }
}
