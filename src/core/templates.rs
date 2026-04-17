use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::config::Config;

/// Get the templates directory for the active site.
/// Returns `<site_root>/.textorium/templates/`.
pub fn templates_dir(config: &Config) -> PathBuf {
    PathBuf::from(&config.site_path)
        .join(".textorium")
        .join("templates")
}

/// List available template names (file stems, without `.yaml` extension).
pub fn list_templates(config: &Config) -> Result<Vec<String>> {
    let dir = templates_dir(config);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in fs::read_dir(&dir)
        .with_context(|| format!("Failed to read templates dir: {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

/// Load a template by name. Returns the frontmatter fields as a HashMap.
/// The template file is `<templates_dir>/<name>.yaml`.
pub fn load_template(config: &Config, name: &str) -> Result<HashMap<String, serde_json::Value>> {
    let path = templates_dir(config).join(format!("{}.yaml", name));
    if !path.exists() {
        anyhow::bail!(
            "Template '{}' not found. Run: textorium templates list",
            name
        );
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read template: {}", path.display()))?;
    let fields: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse template '{}': invalid YAML", name))?;

    // Convert serde_yaml::Value → serde_json::Value
    let mut result = HashMap::new();
    for (key, value) in fields {
        result.insert(key, yaml_value_to_json(value));
    }
    Ok(result)
}

/// Create a new template file with sensible defaults.
pub fn create_template(config: &Config, name: &str) -> Result<PathBuf> {
    // Validate name: no path separators
    if name.contains('/') || name.contains('\\') || name.contains('.') {
        anyhow::bail!("Template name must not contain '/', '\\', or '.'");
    }

    let dir = templates_dir(config);
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create templates dir: {}", dir.display()))?;

    let path = dir.join(format!("{}.yaml", name));
    if path.exists() {
        anyhow::bail!("Template '{}' already exists: {}", name, path.display());
    }

    let default_content = "# Textorium post template — edit fields as needed\n\
        # Fields defined here become the frontmatter scaffold for new posts.\n\
        # 'title' and 'date' are always injected automatically; you can override them here.\n\
        draft: true\n\
        categories: []\n\
        tags: []\n\
        description: \"\"\n";

    fs::write(&path, default_content)
        .with_context(|| format!("Failed to write template: {}", path.display()))?;

    Ok(path)
}

/// Build the frontmatter string for a new post using a template's fields.
/// `title` and `date` are always injected; template fields are merged in.
/// Returns a YAML frontmatter block (without --- delimiters).
pub fn frontmatter_from_template(
    title: &str,
    date_str: &str,
    template_fields: &HashMap<String, serde_json::Value>,
    category_override: Option<&str>,
    tags_override: Option<&[String]>,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!("title: \"{}\"", title.replace('"', "\\\"")));
    lines.push(format!("date: {}", date_str));

    // Collect keys from template, excluding title/date (already injected)
    let mut keys: Vec<&String> = template_fields
        .keys()
        .filter(|k| k.as_str() != "title" && k.as_str() != "date")
        .collect();
    keys.sort();

    for key in &keys {
        let value = template_fields.get(*key).unwrap();

        // Apply CLI overrides
        if key.as_str() == "categories" {
            if let Some(cat) = category_override {
                lines.push(
                    format!("categories: [\"{}\"]\n", cat.replace('"', "\\\""))
                        .trim_end_matches('\n')
                        .to_string(),
                );
                continue;
            }
        }
        if key.as_str() == "tags" {
            if let Some(tags) = tags_override {
                if !tags.is_empty() {
                    let quoted: Vec<String> = tags
                        .iter()
                        .map(|t| format!("\"{}\"", t.replace('"', "\\\"")))
                        .collect();
                    lines.push(format!("tags: [{}]", quoted.join(", ")));
                    continue;
                }
            }
        }

        lines.push(format!("{}: {}", key, json_value_to_yaml_scalar(value)));
    }

    // Add CLI overrides not covered by template
    if let Some(cat) = category_override {
        if !template_fields.contains_key("categories") {
            lines.push(
                format!("categories: [\"{}\"]\n", cat.replace('"', "\\\""))
                    .trim_end_matches('\n')
                    .to_string(),
            );
        }
    }
    if let Some(tags) = tags_override {
        if !tags.is_empty() && !template_fields.contains_key("tags") {
            let quoted: Vec<String> = tags
                .iter()
                .map(|t| format!("\"{}\"", t.replace('"', "\\\"")))
                .collect();
            lines.push(format!("tags: [{}]", quoted.join(", ")));
        }
    }

    lines.join("\n")
}

fn json_value_to_yaml_scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => {
            if s.is_empty() || s.contains(':') || s.contains('#') || s.contains('"') {
                format!("\"{}\"", s.replace('"', "\\\""))
            } else {
                s.clone()
            }
        }
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(arr) => {
            if arr.is_empty() {
                "[]".to_string()
            } else {
                let items: Vec<String> = arr
                    .iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => format!("\"{}\"", s.replace('"', "\\\"")),
                        other => other.to_string(),
                    })
                    .collect();
                format!("[{}]", items.join(", "))
            }
        }
        serde_json::Value::Null => "~".to_string(),
        serde_json::Value::Object(_) => "{}".to_string(),
    }
}

fn yaml_value_to_json(value: serde_yaml::Value) -> serde_json::Value {
    match value {
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Bool(b) => serde_json::Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::json!(i)
            } else if let Some(f) = n.as_f64() {
                serde_json::json!(f)
            } else {
                serde_json::Value::Null
            }
        }
        serde_yaml::Value::String(s) => serde_json::Value::String(s),
        serde_yaml::Value::Sequence(seq) => {
            serde_json::Value::Array(seq.into_iter().map(yaml_value_to_json).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .filter_map(|(k, v)| k.as_str().map(|ks| (ks.to_string(), yaml_value_to_json(v))))
                .collect();
            serde_json::Value::Object(obj)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_value_to_json(tagged.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{Config, SsgType};
    use std::fs;

    fn make_config(site_path: &str) -> Config {
        Config {
            site_name: "test".to_string(),
            site_path: site_path.to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            editor: None,
        }
    }

    #[test]
    fn test_list_templates_empty_when_no_dir() {
        let dir = tempfile::tempdir().unwrap();
        let config = make_config(dir.path().to_str().unwrap());
        let names = list_templates(&config).unwrap();
        assert!(names.is_empty());
    }

    #[test]
    fn test_create_and_list_template() {
        let dir = tempfile::tempdir().unwrap();
        let config = make_config(dir.path().to_str().unwrap());

        let path = create_template(&config, "blog").unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with("blog.yaml"));

        let names = list_templates(&config).unwrap();
        assert_eq!(names, vec!["blog"]);
    }

    #[test]
    fn test_create_template_rejects_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let config = make_config(dir.path().to_str().unwrap());
        create_template(&config, "blog").unwrap();
        let result = create_template(&config, "blog");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_create_template_rejects_path_chars() {
        let dir = tempfile::tempdir().unwrap();
        let config = make_config(dir.path().to_str().unwrap());
        assert!(create_template(&config, "blog/evil").is_err());
        assert!(create_template(&config, "blog.evil").is_err());
    }

    #[test]
    fn test_load_template() {
        let dir = tempfile::tempdir().unwrap();
        let config = make_config(dir.path().to_str().unwrap());

        let tmpl_dir = dir.path().join(".textorium/templates");
        fs::create_dir_all(&tmpl_dir).unwrap();
        fs::write(
            tmpl_dir.join("blog.yaml"),
            "draft: true\ncategories: [\"blog\"]\ndescription: \"\"\n",
        )
        .unwrap();

        let fields = load_template(&config, "blog").unwrap();
        assert_eq!(fields.get("draft"), Some(&serde_json::Value::Bool(true)));
        assert!(fields.contains_key("categories"));
        assert!(fields.contains_key("description"));
    }

    #[test]
    fn test_load_template_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let config = make_config(dir.path().to_str().unwrap());
        let result = load_template(&config, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_frontmatter_from_template() {
        let mut fields = HashMap::new();
        fields.insert("draft".to_string(), serde_json::Value::Bool(true));
        fields.insert(
            "description".to_string(),
            serde_json::Value::String(String::new()),
        );

        let fm = frontmatter_from_template(
            "My Post",
            "2026-04-17T00:00:00Z",
            &fields,
            Some("blog"),
            None,
        );

        assert!(fm.contains("title: \"My Post\""));
        assert!(fm.contains("draft: true"));
        assert!(fm.contains("categories: [\"blog\"]"));
    }
}
