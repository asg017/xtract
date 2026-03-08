use anyhow::{bail, Context, Result};
use markdown::mdast::Node;
use serde::Deserialize;

#[derive(Debug)]
pub struct ExtractSection {
    pub name: String,
    pub prompt: String,
    pub schema: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct Frontmatter {
    /// Name of the section to use as a page classifier
    pub page_classifier: Option<String>,
    /// Crop region for classifier images (percentages 0-100)
    pub classifier_crop: Option<CropRegion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CropRegion {
    /// Percentage from top edge (default 0)
    #[serde(default)]
    pub top: f32,
    /// Percentage from left edge (default 0)
    #[serde(default)]
    pub left: f32,
    /// Width as percentage (default 100)
    #[serde(default = "default_100")]
    pub width: f32,
    /// Height as percentage (default 100)
    #[serde(default = "default_100")]
    pub height: f32,
}

fn default_100() -> f32 {
    100.0
}

#[derive(Debug)]
pub struct ParseResult {
    pub frontmatter: Frontmatter,
    pub sections: Vec<ExtractSection>,
}

/// Strip YAML frontmatter (between --- delimiters) from markdown text.
/// Returns (frontmatter_yaml, remaining_markdown).
fn strip_frontmatter(md_text: &str) -> (&str, &str) {
    let trimmed = md_text.trim_start();
    if !trimmed.starts_with("---") {
        return ("", md_text);
    }

    // Find the opening ---
    let after_open = &trimmed[3..];
    // Must be followed by a newline
    let after_open = if let Some(rest) = after_open.strip_prefix("\r\n") {
        rest
    } else if let Some(rest) = after_open.strip_prefix('\n') {
        rest
    } else {
        return ("", md_text);
    };

    // Find closing ---
    if let Some(end) = after_open.find("\n---") {
        let yaml = &after_open[..end];
        let rest_start = end + 4; // skip \n---
        let rest = if rest_start < after_open.len() {
            &after_open[rest_start..]
        } else {
            ""
        };
        (yaml, rest)
    } else {
        ("", md_text)
    }
}

pub fn parse(md_text: &str) -> Result<ParseResult> {
    let (yaml_str, md_body) = strip_frontmatter(md_text);

    let frontmatter: Frontmatter = if yaml_str.is_empty() {
        Frontmatter::default()
    } else {
        serde_yaml::from_str(yaml_str)
            .context("Failed to parse YAML frontmatter")?
    };

    // Validate frontmatter
    if frontmatter.classifier_crop.is_some() && frontmatter.page_classifier.is_none() {
        bail!("classifier_crop requires page_classifier to be set");
    }
    if let Some(ref crop) = frontmatter.classifier_crop {
        if crop.top < 0.0 || crop.left < 0.0 || crop.width <= 0.0 || crop.height <= 0.0 {
            bail!("classifier_crop values must be non-negative (width/height must be > 0)");
        }
        if crop.top + crop.height > 100.0 || crop.left + crop.width > 100.0 {
            bail!("classifier_crop region exceeds 100%");
        }
    }

    let sections = parse_sections(md_body)?;

    // Validate page_classifier references a real section
    if let Some(ref classifier_name) = frontmatter.page_classifier {
        let found = sections
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(classifier_name));
        if !found {
            let names: Vec<_> = sections.iter().map(|s| s.name.as_str()).collect();
            bail!(
                "page_classifier references unknown section \"{classifier_name}\"; \
                 available sections: {}",
                names.join(", ")
            );
        }
    }

    Ok(ParseResult {
        frontmatter,
        sections,
    })
}

fn parse_sections(md_text: &str) -> Result<Vec<ExtractSection>> {
    let tree = markdown::to_mdast(md_text, &markdown::ParseOptions::default())
        .map_err(|e| anyhow::anyhow!("Failed to parse markdown: {e}"))?;

    let Node::Root(root) = tree else {
        bail!("Unexpected markdown parse result");
    };

    // Split children into sections by headings.
    // Content before the first heading goes into a default section.
    let mut sections = Vec::new();
    let mut current_name = String::new();
    let mut current_nodes: Vec<&Node> = Vec::new();

    for node in &root.children {
        if let Node::Heading(heading) = node {
            if (!current_nodes.is_empty() || !current_name.is_empty())
                && let Some(section) = build_section(&current_name, &current_nodes)?
            {
                sections.push(section);
            }
            current_name = extract_heading_text(heading);
            current_nodes = Vec::new();
        } else {
            current_nodes.push(node);
        }
    }

    // Final section
    if (!current_nodes.is_empty() || !current_name.is_empty())
        && let Some(section) = build_section(&current_name, &current_nodes)?
    {
        sections.push(section);
    }

    if sections.is_empty() {
        bail!("Markdown file has no sections with a ```schema block");
    }

    Ok(sections)
}

fn build_section(name: &str, nodes: &[&Node]) -> Result<Option<ExtractSection>> {
    let mut schema = None;
    let mut prompt_parts = Vec::new();

    for node in nodes {
        match node {
            Node::Code(code) => {
                if matches!(code.lang.as_deref(), Some("schema" | "js")) {
                    if schema.is_some() {
                        bail!(
                            "Section \"{}\" has multiple ```schema blocks; only one is allowed",
                            if name.is_empty() { "(untitled)" } else { name }
                        );
                    }
                    schema = Some(code.value.clone());
                } else {
                    // Non-schema code blocks become part of the prompt
                    prompt_parts.push(format!("```{}\n{}\n```", code.lang.as_deref().unwrap_or(""), code.value));
                }
            }
            _ => {
                let text = collect_text(node);
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    prompt_parts.push(trimmed.to_string());
                }
            }
        }
    }

    let Some(schema) = schema else {
        // No schema block in this section, skip it
        return Ok(None);
    };

    let prompt = prompt_parts.join("\n\n");
    if prompt.is_empty() {
        bail!(
            "Section \"{}\" has a ```schema block but no prompt text",
            if name.is_empty() { "(untitled)" } else { name }
        );
    }

    Ok(Some(ExtractSection {
        name: name.to_string(),
        prompt,
        schema,
    }))
}

fn extract_heading_text(heading: &markdown::mdast::Heading) -> String {
    let mut text = String::new();
    for child in &heading.children {
        text.push_str(&collect_text(child));
    }
    text.trim().to_string()
}

fn collect_text(node: &Node) -> String {
    match node {
        Node::Text(t) => t.value.clone(),
        Node::InlineCode(c) => c.value.clone(),
        Node::Emphasis(e) => e.children.iter().map(collect_text).collect(),
        Node::Strong(s) => s.children.iter().map(collect_text).collect(),
        Node::Paragraph(p) => p.children.iter().map(collect_text).collect(),
        Node::Link(l) => l.children.iter().map(collect_text).collect(),
        Node::List(l) => l
            .children
            .iter()
            .map(|item| format!("- {}", collect_text(item)))
            .collect::<Vec<_>>()
            .join("\n"),
        Node::ListItem(li) => li.children.iter().map(collect_text).collect(),
        Node::Blockquote(bq) => bq.children.iter().map(collect_text).collect(),
        Node::ThematicBreak(_) | Node::Break(_) => "\n".to_string(),
        _ => String::new(),
    }
}

pub fn resolve_section<'a>(
    sections: &'a [ExtractSection],
    name: Option<&str>,
) -> Result<&'a ExtractSection> {
    if sections.len() == 1 {
        if let Some(name) = name {
            let section = &sections[0];
            if !section.name.is_empty() && !section.name.eq_ignore_ascii_case(name) {
                bail!(
                    "No section named \"{name}\"; available: \"{}\"",
                    section.name
                );
            }
        }
        return Ok(&sections[0]);
    }

    // Multiple sections: --name is required
    let Some(name) = name else {
        let names: Vec<_> = sections.iter().map(|s| s.name.as_str()).collect();
        bail!(
            "Markdown has {} sections; use --name to select one: {}",
            sections.len(),
            names.join(", ")
        );
    };

    sections
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(name))
        .with_context(|| {
            let names: Vec<_> = sections.iter().map(|s| s.name.as_str()).collect();
            format!(
                "No section named \"{name}\"; available: {}",
                names.join(", ")
            )
        })
}

pub fn find_section<'a>(
    sections: &'a [ExtractSection],
    name: &str,
) -> Result<&'a ExtractSection> {
    sections
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(name))
        .with_context(|| {
            let names: Vec<_> = sections.iter().map(|s| s.name.as_str()).collect();
            format!(
                "No section named \"{name}\"; available: {}",
                names.join(", ")
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_section_no_heading() {
        let md = r#"
Extract all the line items from this receipt.

```schema
z.object({ items: z.array(z.object({ name: z.string(), price: z.number() })) })
```
"#;
        let result = parse(md).unwrap();
        assert_eq!(result.sections.len(), 1);
        assert_eq!(result.sections[0].name, "");
        assert!(result.sections[0].prompt.contains("line items"));
        assert!(result.sections[0].schema.contains("z.object"));
        assert!(result.frontmatter.page_classifier.is_none());
    }

    #[test]
    fn multiple_sections() {
        let md = r#"
# Receipt

Extract all line items from this receipt.

```schema
z.object({ items: z.array(z.string()) })
```

# Invoice

Extract the invoice number and total.

```schema
z.object({ invoice_number: z.string(), total: z.number() })
```
"#;
        let result = parse(md).unwrap();
        assert_eq!(result.sections.len(), 2);
        assert_eq!(result.sections[0].name, "Receipt");
        assert_eq!(result.sections[1].name, "Invoice");
    }

    #[test]
    fn resolve_by_name() {
        let md = r#"
# Foo

Do foo things.

```schema
{}
```

# Bar

Do bar things.

```schema
{}
```
"#;
        let result = parse(md).unwrap();
        let s = resolve_section(&result.sections, Some("bar")).unwrap();
        assert_eq!(s.name, "Bar");
    }

    #[test]
    fn missing_schema_block() {
        let md = "Just some text without a schema block.\n";
        let result = parse(md);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no sections"));
    }

    #[test]
    fn no_prompt_text() {
        let md = "```schema\n{}\n```\n";
        let result = parse(md);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no prompt"));
    }

    #[test]
    fn frontmatter_classifier() {
        let md = r#"---
page_classifier: PageType
classifier_crop:
  height: 20
---

# PageType

Determine the type of this page.

```schema
z.object({ page_type: z.enum(["Invoice", "Receipt"]) })
```

# Invoice

Extract invoice data.

```schema
z.object({ total: z.number() })
```

# Receipt

Extract receipt data.

```schema
z.object({ store: z.string() })
```
"#;
        let result = parse(md).unwrap();
        assert_eq!(
            result.frontmatter.page_classifier.as_deref(),
            Some("PageType")
        );
        let crop = result.frontmatter.classifier_crop.unwrap();
        assert_eq!(crop.top, 0.0);
        assert_eq!(crop.height, 20.0);
        assert_eq!(crop.width, 100.0);
        assert_eq!(result.sections.len(), 3);
    }

    #[test]
    fn frontmatter_bad_classifier_ref() {
        let md = r#"---
page_classifier: NonExistent
---

# Foo

Do things.

```schema
{}
```
"#;
        let result = parse(md);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unknown section"));
    }

    #[test]
    fn frontmatter_crop_without_classifier() {
        let md = r#"---
classifier_crop:
  height: 20
---

# Foo

Do things.

```schema
{}
```
"#;
        let result = parse(md);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requires"));
    }
}
