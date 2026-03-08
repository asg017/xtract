mod cli;
mod commands;
mod js_runner;
mod markdown;
mod pages;
mod progress;
mod sqlite;

use std::sync::{Arc, Mutex};

use clap::Parser;
use commands::extract::{self, ExtractArgs, ExtractResult};
use progress::{Progress, Worker};

/// Result of processing a single page, ready for SQLite insertion.
struct PageResult {
    page_num: u32,
    result: ExtractResult,
    classifier_data: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();

    match cli.command {
        cli::Command::Schema { file } => commands::schema::run(&file),
        cli::Command::Check { file } => commands::check::run(&file),
        cli::Command::Extract {
            input,
            schema,
            prompt,
            model,
            provider,
            page,
            pages: pages_spec,
            screenshot,
            name,
            output,
            concurrency,
        } => {
            let provider_config = extract::resolve_provider(&provider)?;
            let is_sqlite = output
                .as_ref()
                .is_some_and(|p| p.extension().is_some_and(|e| e == "db"));

            let is_clipboard = extract::is_clipboard(&input);

            let is_pdf = !is_clipboard && matches!(
                input.extension().and_then(|e| e.to_str()),
                Some("pdf" | "PDF")
            );

            let is_md = !is_clipboard && matches!(
                input.extension().and_then(|e| e.to_str()),
                Some("md" | "markdown")
            );

            // Check if markdown has a page_classifier (needs early parse)
            let md_parsed = if is_md {
                let md_text = std::fs::read_to_string(&input)?;
                Some(markdown::parse(&md_text)?)
            } else {
                None
            };

            let has_classifier = md_parsed
                .as_ref()
                .is_some_and(|p| p.frontmatter.page_classifier.is_some());

            // Classifier mode
            if has_classifier {
                let image_path = schema.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "When using a markdown file with page_classifier, provide the PDF path \
                         as the second argument:\n  schema-extract extract recipe.md document.pdf"
                    )
                })?;

                let is_input_pdf = matches!(
                    image_path.extension().and_then(|e| e.to_str()),
                    Some("pdf" | "PDF")
                );
                if !is_input_pdf {
                    anyhow::bail!("page_classifier requires a PDF input");
                }
                if !is_sqlite {
                    anyhow::bail!("page_classifier requires SQLite output (-o <path>.db)");
                }

                let parsed = md_parsed.as_ref().unwrap();
                let classifier_name = parsed.frontmatter.page_classifier.as_deref().unwrap();
                let crop = parsed.frontmatter.classifier_crop.as_ref();

                let page_count = extract::pdf_page_count(image_path)? as u32;
                let page_list: Vec<u32> = if let Some(ref spec) = pages_spec {
                    pages::parse_page_spec(spec, page_count)?
                } else if let Some(p) = page {
                    vec![p]
                } else {
                    (1..=page_count).collect()
                };

                let db_path = output.as_ref().unwrap();
                let total = page_list.len();
                let nc = concurrency.min(total).max(1);

                let (results, prog) = run_concurrent(nc, &page_list, |page_num, w| {
                    w.status(page_num, "rendering page");
                    let (image_bytes, _mime) =
                        extract::get_image_bytes(image_path, Some(page_num), true)?;

                    let classifier_image = if let Some(crop) = crop {
                        w.status(page_num, "cropping for classifier");
                        extract::crop_image(&image_bytes, crop)?
                    } else {
                        image_bytes
                    };

                    w.status(page_num, &format!("classifying via {provider}"));
                    let classifier_section =
                        markdown::find_section(&parsed.sections, classifier_name)?;
                    let classifier_schema =
                        extract::parse_schema_content(&classifier_section.schema)?;
                    let (classifier_json, _) = extract::call_api(
                        &classifier_image,
                        "image/png",
                        &classifier_schema,
                        &classifier_section.prompt,
                        &model,
                        &provider_config,
                    )?;

                    let classifier_value: serde_json::Value =
                        serde_json::from_str(&classifier_json)?;
                    let page_type = classifier_value
                        .get("page_type")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "p{page_num}: classifier did not return \"page_type\"; got: {classifier_json}"
                            )
                        })?
                        .to_string();

                    if page_type.eq_ignore_ascii_case(classifier_name) {
                        anyhow::bail!(
                            "p{page_num}: classifier returned page_type=\"{page_type}\" \
                             which is the classifier itself (infinite loop)"
                        );
                    }

                    let target_section =
                        markdown::find_section(&parsed.sections, &page_type)
                            .map_err(|_| {
                                let valid: Vec<_> = parsed
                                    .sections
                                    .iter()
                                    .filter(|s| !s.name.eq_ignore_ascii_case(classifier_name))
                                    .map(|s| s.name.as_str())
                                    .collect();
                                anyhow::anyhow!(
                                    "p{page_num}: classifier returned page_type=\"{page_type}\" \
                                     which is not a known section; valid: {}",
                                    valid.join(", ")
                                )
                            })?;

                    w.status(page_num, &format!("extracting \"{page_type}\" via {provider}"));
                    let (full_image, full_mime) =
                        extract::get_image_bytes(image_path, Some(page_num), screenshot)?;
                    let target_schema =
                        extract::parse_schema_content(&target_section.schema)?;
                    let (data, timing) = extract::call_api(
                        &full_image,
                        &full_mime,
                        &target_schema,
                        &target_section.prompt,
                        &model,
                        &provider_config,
                    )?;

                    Ok(PageResult {
                        page_num,
                        result: ExtractResult {
                            data,
                            json_schema: target_schema,
                            prompt: target_section.prompt.clone(),
                            image_bytes: full_image,
                            image_mime: full_mime,
                            timing,
                        },
                        classifier_data: Some(classifier_json),
                    })
                })?;

                for pr in &results {
                    sqlite::insert(
                        db_path,
                        &pr.result,
                        &sqlite::InsertOpts {
                            input_file: image_path,
                            page: Some(pr.page_num),
                            page_count: Some(page_count),
                            model: &model,
                            classifier_data: pr.classifier_data.as_deref(),
                        },
                    )?;
                }

                prog.finish(&format!(
                    "Classified and extracted {total} pages into {}",
                    db_path.display()
                ));
                return Ok(());
            }

            // --- Non-classifier flow ---

            let page_list: Vec<u32> = if let Some(ref spec) = pages_spec {
                if !is_pdf {
                    anyhow::bail!("--pages is only valid for PDF files");
                }
                let count = extract::pdf_page_count(&input)? as u32;
                pages::parse_page_spec(spec, count)?
            } else if let Some(p) = page {
                vec![p]
            } else if is_pdf && is_sqlite {
                let count = extract::pdf_page_count(&input)? as u32;
                (1..=count).collect()
            } else {
                vec![]
            };

            if page_list.len() > 1 && !is_sqlite {
                anyhow::bail!("Multiple pages require SQLite output (-o <path>.db)");
            }

            // Single-page path
            if page_list.len() <= 1 {
                let effective_page = page_list.first().copied().or(page);
                let args = ExtractArgs {
                    input: &input,
                    schema: schema.as_deref(),
                    prompt: prompt.as_deref(),
                    model: &model,
                    provider: &provider_config,
                    page: effective_page,
                    screenshot,
                    name: name.as_deref(),
                };
                let result = extract::run(&args)?;

                eprintln!(
                    "Extracted in {}ms ({} → {})",
                    result.timing.elapsed_ms, result.timing.started_at, result.timing.finished_at,
                );

                if is_sqlite {
                    let db_path = output.as_ref().unwrap();
                    let page_count = if is_pdf {
                        Some(extract::pdf_page_count(&input)? as u32)
                    } else {
                        None
                    };
                    sqlite::insert(
                        db_path,
                        &result,
                        &sqlite::InsertOpts {
                            input_file: &input,
                            page: effective_page,
                            page_count,
                            model: &model,
                            classifier_data: None,
                        },
                    )?;
                    eprintln!("Inserted into {}", db_path.display());
                } else if let Some(path) = &output {
                    std::fs::write(path, &result.data)?;
                    eprintln!("Wrote {}", path.display());
                } else {
                    println!("{}", result.data);
                }

                return Ok(());
            }

            // Multi-page SQLite path (concurrent)
            let db_path = output.as_ref().unwrap();
            let total = page_list.len();
            let nc = concurrency.min(total).max(1);

            // Pre-compile schema once
            let json_schema: serde_json::Value = if is_md {
                let md_text = std::fs::read_to_string(&input)?;
                let parsed = markdown::parse(&md_text)?;
                let section =
                    markdown::resolve_section(&parsed.sections, name.as_deref())?;
                extract::parse_schema_content(&section.schema)?
            } else {
                let schema_path = schema.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Schema file is required for non-markdown extraction")
                })?;
                serde_json::from_str(&js_runner::run(schema_path)?)?
            };

            let prompt_str = prompt
                .as_deref()
                .unwrap_or("Extract the structured data from this image.");

            let (results, prog) = run_concurrent(nc, &page_list, |page_num, w| {
                w.status(page_num, "extracting image");
                let (image_bytes, mime) =
                    extract::get_image_bytes(&input, Some(page_num), screenshot)?;

                w.status(page_num, &format!("calling {provider}"));
                let (data, timing) =
                    extract::call_api(&image_bytes, &mime, &json_schema, prompt_str, &model, &provider_config)?;

                Ok(PageResult {
                    page_num,
                    result: ExtractResult {
                        data,
                        json_schema: json_schema.clone(),
                        prompt: prompt_str.to_string(),
                        image_bytes,
                        image_mime: mime,
                        timing,
                    },
                    classifier_data: None,
                })
            })?;

            let page_count = if is_pdf {
                Some(extract::pdf_page_count(&input)? as u32)
            } else {
                None
            };
            for pr in &results {
                sqlite::insert(
                    db_path,
                    &pr.result,
                    &sqlite::InsertOpts {
                        input_file: &input,
                        page: Some(pr.page_num),
                        page_count,
                        model: &model,
                        classifier_data: None,
                    },
                )?;
            }

            prog.finish(&format!("Extracted {total} pages into {}", db_path.display()));

            Ok(())
        }
    }
}

/// Process pages concurrently with up to `nc` worker threads.
/// Each worker grabs the next unprocessed page from a shared queue.
/// Returns results sorted by page number.
fn run_concurrent<F>(
    nc: usize,
    page_list: &[u32],
    process: F,
) -> anyhow::Result<(Vec<PageResult>, Progress)>
where
    F: Fn(u32, &Worker<'_>) -> anyhow::Result<PageResult> + Sync,
{
    let total = page_list.len();
    let prog = Progress::new(total, nc);
    let work = Mutex::new(page_list.iter().copied());
    let results: Arc<Mutex<Vec<PageResult>>> = Arc::new(Mutex::new(Vec::with_capacity(total)));
    let first_error: Arc<Mutex<Option<anyhow::Error>>> = Arc::new(Mutex::new(None));

    std::thread::scope(|s| {
        for worker_idx in 0..nc {
            let work = &work;
            let results = &results;
            let first_error = &first_error;
            let prog = &prog;
            let process = &process;

            s.spawn(move || {
                let w = prog.worker(worker_idx);
                loop {
                    if first_error.lock().unwrap().is_some() {
                        break;
                    }

                    let page_num = {
                        let mut iter = work.lock().unwrap();
                        iter.next()
                    };
                    let Some(page_num) = page_num else {
                        break;
                    };

                    match process(page_num, &w) {
                        Ok(pr) => {
                            w.complete_page();
                            results.lock().unwrap().push(pr);
                        }
                        Err(e) => {
                            let mut err = first_error.lock().unwrap();
                            if err.is_none() {
                                *err = Some(e);
                            }
                            break;
                        }
                    }
                }
            });
        }
    });

    let err = first_error.lock().unwrap().take();
    if let Some(e) = err {
        prog.finish_err("Failed");
        return Err(e);
    }

    let mut out = Arc::try_unwrap(results)
        .map_err(|_| anyhow::anyhow!("Failed to unwrap results"))?
        .into_inner()
        .map_err(|e| anyhow::anyhow!("Mutex poisoned: {e}"))?;
    out.sort_by_key(|r| r.page_num);

    Ok((out, prog))
}
