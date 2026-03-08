use std::path::Path;

use crate::commands::extract;
use crate::markdown;

pub fn run(file: &Path) -> anyhow::Result<()> {
    let md_text = std::fs::read_to_string(file)?;

    // 1. Parse markdown + frontmatter
    let parsed = match markdown::parse(&md_text) {
        Ok(p) => {
            eprintln!("  frontmatter: ok");
            p
        }
        Err(e) => {
            eprintln!("  frontmatter/parse: FAIL - {e}");
            return Err(e);
        }
    };

    let mut has_errors = false;

    // 2. Check each section's schema compiles
    for section in &parsed.sections {
        let label = if section.name.is_empty() {
            "(untitled)".to_string()
        } else {
            section.name.clone()
        };

        match extract::parse_schema_content(&section.schema) {
            Ok(schema) => {
                let title = schema
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("(no title)");
                eprintln!("  section \"{label}\": ok (schema title: {title})");
            }
            Err(e) => {
                eprintln!("  section \"{label}\": FAIL - {e}");
                has_errors = true;
            }
        }
    }

    // 3. Check classifier references
    if let Some(ref classifier_name) = parsed.frontmatter.page_classifier {
        let classifier_section = parsed
            .sections
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(classifier_name));

        match classifier_section {
            Some(section) => {
                // Try to compile the classifier schema and check it has page_type
                match extract::parse_schema_content(&section.schema) {
                    Ok(schema) => {
                        // Check the schema has a page_type property
                        let has_page_type = schema
                            .pointer("/properties/page_type")
                            .is_some();
                        if has_page_type {
                            eprintln!("  classifier \"{classifier_name}\": ok");
                        } else {
                            eprintln!(
                                "  classifier \"{classifier_name}\": WARNING - \
                                 schema has no \"page_type\" property"
                            );
                        }

                        // Check enum values (if present) reference valid sections
                        if let Some(enum_values) = schema
                            .pointer("/properties/page_type/enum")
                            .and_then(|v| v.as_array())
                        {
                            let section_names: Vec<_> = parsed
                                .sections
                                .iter()
                                .map(|s| s.name.as_str())
                                .collect();

                            for val in enum_values {
                                if let Some(name) = val.as_str() {
                                    if name.eq_ignore_ascii_case(classifier_name) {
                                        eprintln!(
                                            "  classifier enum: FAIL - \"{name}\" references \
                                             the classifier itself (would cause infinite loop)"
                                        );
                                        has_errors = true;
                                    } else if !section_names
                                        .iter()
                                        .any(|s| s.eq_ignore_ascii_case(name))
                                    {
                                        eprintln!(
                                            "  classifier enum: FAIL - \"{name}\" is not a \
                                             known section"
                                        );
                                        has_errors = true;
                                    }
                                }
                            }
                            if !has_errors {
                                let names: Vec<_> = enum_values
                                    .iter()
                                    .filter_map(|v| v.as_str())
                                    .collect();
                                eprintln!(
                                    "  classifier types: ok ({})",
                                    names.join(", ")
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "  classifier \"{classifier_name}\": FAIL - schema error: {e}"
                        );
                        has_errors = true;
                    }
                }
            }
            None => {
                // This should already be caught by markdown::parse, but just in case
                eprintln!(
                    "  classifier: FAIL - section \"{classifier_name}\" not found"
                );
                has_errors = true;
            }
        }

        if let Some(ref crop) = parsed.frontmatter.classifier_crop {
            eprintln!(
                "  classifier_crop: top={}%, left={}%, {}%x{}%",
                crop.top, crop.left, crop.width, crop.height
            );
        }
    }

    // Summary
    eprintln!();
    if has_errors {
        anyhow::bail!("Validation failed");
    } else {
        eprintln!(
            "{} section(s) validated successfully",
            parsed.sections.len()
        );
        Ok(())
    }
}
