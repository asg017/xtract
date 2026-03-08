use base64::Engine;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use crate::js_runner;
use crate::markdown;
use crate::markdown::CropRegion;
use crate::pages;
use crate::progress::{Progress, Worker};
use crate::sqlite;

pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: Option<String>,
}

pub fn resolve_provider(provider: &str) -> anyhow::Result<ProviderConfig> {
    match provider {
        "openrouter" => {
            let api_key = std::env::var("OPENROUTER_API_KEY")
                .map_err(|_| anyhow::anyhow!("OPENROUTER_API_KEY environment variable is required"))?;
            Ok(ProviderConfig {
                base_url: "https://openrouter.ai/api/v1".to_string(),
                api_key: Some(api_key),
            })
        }
        "llamabarn" => Ok(ProviderConfig {
            base_url: "http://localhost:2276/v1".to_string(),
            api_key: None,
        }),
        url if url.starts_with("http://") || url.starts_with("https://") => {
            let api_key = std::env::var("LLM_API_KEY").ok();
            Ok(ProviderConfig {
                base_url: url.trim_end_matches('/').to_string(),
                api_key,
            })
        }
        other => anyhow::bail!(
            "Unknown provider \"{other}\". Use \"openrouter\", \"llamabarn\", or a custom URL (http://...)"
        ),
    }
}

pub struct ExtractArgs<'a> {
    pub schema: &'a Path,
    pub input: &'a Path,
    pub prompt: Option<&'a str>,
    pub model: &'a str,
    pub provider: &'a ProviderConfig,
    pub page: Option<u32>,
    pub screenshot: bool,
    pub name: Option<&'a str>,
}

pub struct ApiTiming {
    pub started_at: String,
    pub finished_at: String,
    pub elapsed_ms: u64,
}

pub struct ExtractResult {
    /// The extracted JSON data (pretty-printed)
    pub data: String,
    /// The compiled JSON Schema used
    pub json_schema: serde_json::Value,
    /// The prompt sent to the model
    pub prompt: String,
    /// The image bytes sent to the model
    pub image_bytes: Vec<u8>,
    /// MIME type of the image
    pub image_mime: String,
    /// Timing info for the API call
    pub timing: ApiTiming,
}

pub fn is_md_schema(schema: &Path) -> bool {
    matches!(
        schema.extension().and_then(|e| e.to_str()),
        Some("md" | "markdown")
    )
}

pub fn run(args: &ExtractArgs) -> anyhow::Result<ExtractResult> {
    if is_md_schema(args.schema) {
        return run_markdown(args);
    }

    let prompt = args
        .prompt
        .unwrap_or("Extract the structured data from this image.")
        .to_string();

    let (image_bytes, mime) = get_image_bytes(args.input, args.page, args.screenshot)?;
    let json_schema: serde_json::Value = serde_json::from_str(&js_runner::run(args.schema)?)?;

    let (data, timing) = call_api(&image_bytes, &mime, &json_schema, &prompt, args.model, args.provider)?;
    Ok(ExtractResult {
        data,
        json_schema,
        prompt,
        image_bytes,
        image_mime: mime,
        timing,
    })
}

fn run_markdown(args: &ExtractArgs) -> anyhow::Result<ExtractResult> {
    let md_text = std::fs::read_to_string(args.schema)?;
    let parsed = markdown::parse(&md_text)?;
    let section = markdown::resolve_section(&parsed.sections, args.name)?;

    let json_schema = parse_schema_content(&section.schema)?;

    let (image_bytes, mime) = get_image_bytes(args.input, args.page, args.screenshot)?;
    let prompt = section.prompt.to_string();

    let (data, timing) = call_api(&image_bytes, &mime, &json_schema, &prompt, args.model, args.provider)?;
    Ok(ExtractResult {
        data,
        json_schema,
        prompt,
        image_bytes,
        image_mime: mime,
        timing,
    })
}

pub fn parse_schema_content(content: &str) -> anyhow::Result<serde_json::Value> {
    let trimmed = content.trim();

    // Try raw JSON Schema first
    if trimmed.starts_with('{') && let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(v);
    }

    // Otherwise treat as Zod JS
    let js_content = if trimmed.contains("export default") {
        trimmed.to_string()
    } else {
        format!("export default {trimmed}")
    };
    let result = js_runner::run_source(&js_content)?;
    Ok(serde_json::from_str(&result)?)
}

pub fn crop_image(image_bytes: &[u8], crop: &CropRegion) -> anyhow::Result<Vec<u8>> {
    use image::GenericImageView;
    use std::io::Cursor;

    let img = image::load_from_memory(image_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode image for cropping: {e}"))?;
    let (w, h) = img.dimensions();

    let x = (w as f32 * crop.left / 100.0) as u32;
    let y = (h as f32 * crop.top / 100.0) as u32;
    let cw = (w as f32 * crop.width / 100.0).min((w - x) as f32) as u32;
    let ch = (h as f32 * crop.height / 100.0).min((h - y) as f32) as u32;

    let cropped = img.crop_imm(x, y, cw, ch);

    let mut buf = Cursor::new(Vec::new());
    cropped.write_to(&mut buf, image::ImageFormat::Png)?;
    Ok(buf.into_inner())
}

pub fn pdf_page_count(input: &Path) -> anyhow::Result<usize> {
    use pdf_lib_rs::api::PdfDocument;
    let pdf_bytes = std::fs::read(input)?;
    let doc = PdfDocument::load(&pdf_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse PDF: {e}"))?;
    Ok(doc.get_page_refs().len())
}

pub fn is_clipboard(input: &Path) -> bool {
    input.as_os_str() == "clipboard"
}

pub fn get_image_bytes(
    input: &Path,
    page: Option<u32>,
    screenshot: bool,
) -> anyhow::Result<(Vec<u8>, String)> {
    if is_clipboard(input) {
        if page.is_some() {
            anyhow::bail!("--page is not valid with clipboard input");
        }
        if screenshot {
            anyhow::bail!("--screenshot is not valid with clipboard input");
        }
        return read_clipboard_image();
    }

    let is_pdf = matches!(
        input.extension().and_then(|e| e.to_str()),
        Some("pdf" | "PDF")
    );

    if is_pdf {
        get_pdf_image(input, page, screenshot)
    } else {
        if page.is_some() {
            anyhow::bail!("--page is only valid for PDF files");
        }
        if screenshot {
            anyhow::bail!("--screenshot is only valid for PDF files");
        }
        let bytes = std::fs::read(input)?;
        let mime = match input.extension().and_then(|e| e.to_str()) {
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            Some("gif") => "image/gif",
            _ => "image/png",
        };
        Ok((bytes, mime.to_string()))
    }
}

fn read_clipboard_image() -> anyhow::Result<(Vec<u8>, String)> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| anyhow::anyhow!("Failed to access clipboard: {e}"))?;
    let img_data = clipboard
        .get_image()
        .map_err(|_| anyhow::anyhow!("No image found in clipboard"))?;

    let img = image::RgbaImage::from_raw(
        img_data.width as u32,
        img_data.height as u32,
        img_data.bytes.into_owned(),
    )
    .ok_or_else(|| anyhow::anyhow!("Failed to construct image from clipboard data"))?;

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)?;
    Ok((buf.into_inner(), "image/png".to_string()))
}

fn now_iso() -> String {
    let output = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok();
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn call_api(
    image_bytes: &[u8],
    mime: &str,
    json_schema: &serde_json::Value,
    prompt: &str,
    model: &str,
    provider: &ProviderConfig,
) -> anyhow::Result<(String, ApiTiming)> {
    let image_b64 = base64::prelude::BASE64_STANDARD.encode(image_bytes);

    let schema_name = json_schema
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("Schema");

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": prompt },
                    {
                        "type": "image_url",
                        "image_url": {
                            "url": format!("data:{mime};base64,{image_b64}")
                        }
                    }
                ]
            }
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": schema_name,
                "strict": true,
                "schema": json_schema
            }
        }
    });

    let url = format!("{}/chat/completions", provider.base_url);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut req = agent.post(&url)
        .header("Content-Type", "application/json");
    if let Some(ref api_key) = provider.api_key {
        req = req.header("Authorization", &format!("Bearer {api_key}"));
    }

    let started_at = now_iso();
    let start = Instant::now();

    let mut resp = req.send_json(&body)?;
    let status = resp.status().as_u16();
    if status < 200 || status >= 300 {
        let body_text = resp.body_mut().read_to_string().unwrap_or_default();
        // Try to pretty-print JSON error, otherwise show raw body
        let detail = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body_text) {
            serde_json::to_string_pretty(&v).unwrap_or(body_text)
        } else {
            body_text
        };
        let img_kb = image_bytes.len() / 1024;
        anyhow::bail!(
            "API returned HTTP {status} (model={model}, image={img_kb}KB {mime}):\n{detail}"
        );
    }
    let response: serde_json::Value = resp.body_mut().read_json()?;

    let elapsed = start.elapsed();
    let finished_at = now_iso();

    let content = response["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Unexpected API response: {response}"))?;

    let parsed: serde_json::Value = serde_json::from_str(content)?;
    let timing = ApiTiming {
        started_at,
        finished_at,
        elapsed_ms: elapsed.as_millis() as u64,
    };
    Ok((serde_json::to_string_pretty(&parsed)?, timing))
}

fn get_pdf_image(
    pdf_path: &Path,
    page: Option<u32>,
    screenshot: bool,
) -> anyhow::Result<(Vec<u8>, String)> {
    use pdf_lib_rs::api::PdfDocument;
    use pdf_lib_rs::core::objects::{PdfName, PdfObject};

    let pdf_bytes = std::fs::read(pdf_path)?;
    let doc = PdfDocument::load(&pdf_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse PDF: {e}"))?;

    let page_count = doc.get_page_refs().len();

    if page_count > 1 && page.is_none() {
        anyhow::bail!(
            "PDF has {page_count} pages; use --page (-p) to specify which page to use"
        );
    }

    let page_num = page.unwrap_or(1);
    if page_num == 0 || page_num as usize > page_count {
        anyhow::bail!(
            "Page {page_num} out of range (document has {page_count} page(s))"
        );
    }

    if screenshot {
        return take_screenshot(pdf_path, page_num);
    }

    // Extract the first embedded image from the given page
    let ctx = doc.context();
    let page_refs = doc.get_page_refs();
    let page_ref = &page_refs[page_num as usize - 1];

    let Some(PdfObject::Dict(page_dict)) = ctx.lookup(page_ref) else {
        anyhow::bail!("Failed to read page {page_num}");
    };

    let res = match page_dict.get(&PdfName::of("Resources")) {
        Some(obj) => resolve_to_dict(ctx, obj),
        None => {
            if let Some(PdfObject::Ref(parent_ref)) = page_dict.get(&PdfName::of("Parent")) {
                if let Some(PdfObject::Dict(parent)) = ctx.lookup(parent_ref) {
                    parent
                        .get(&PdfName::of("Resources"))
                        .and_then(|o| resolve_to_dict(ctx, o))
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    let Some(res) = res else {
        anyhow::bail!("No resources found on page {page_num}");
    };

    let xobj = res
        .get(&PdfName::of("XObject"))
        .and_then(|o| resolve_to_dict(ctx, o));

    let Some(xobj_dict) = xobj else {
        anyhow::bail!("No images found on page {page_num}");
    };

    // Collect image refs on this page
    let mut image_streams = Vec::new();
    for (_name, value) in xobj_dict.entries() {
        let r = match value {
            PdfObject::Ref(r) => r,
            _ => continue,
        };
        let Some(PdfObject::Stream(s)) = ctx.lookup(r) else {
            continue;
        };
        if let Some(PdfObject::Name(subtype)) = s.dict.get(&PdfName::of("Subtype"))
            && subtype.as_string() == "/Image"
        {
            image_streams.push(s);
        }
    }

    if image_streams.is_empty() {
        anyhow::bail!("No images found on page {page_num}");
    }
    if image_streams.len() > 1 {
        anyhow::bail!(
            "Page {page_num} has {} images; use --screenshot to render the page instead",
            image_streams.len()
        );
    }

    let stream = image_streams[0];
    let filter = stream
        .dict
        .get(&PdfName::of("Filter"))
        .and_then(|o| match o {
            PdfObject::Name(n) => Some(n.as_string().to_string()),
            PdfObject::Array(arr) if arr.size() == 1 => {
                if let Some(PdfObject::Name(n)) = arr.get(0) {
                    Some(n.as_string().to_string())
                } else {
                    None
                }
            }
            _ => None,
        });

    let (bytes, mime) = match filter.as_deref() {
        Some("/DCTDecode") => (stream.contents.clone(), "image/jpeg".to_string()),
        Some("/JPXDecode") => (stream.contents.clone(), "image/jp2".to_string()),
        Some("/FlateDecode") => {
            // Decompress and re-encode as PNG
            let tmp = tempfile::NamedTempFile::new()?;
            let tmp_path = tmp.path().to_path_buf();
            // Use pdf_utils-style PNG writing
            write_stream_as_png(stream, &tmp_path)?;
            let bytes = std::fs::read(&tmp_path)?;
            (bytes, "image/png".to_string())
        }
        _ => (stream.contents.clone(), "image/png".to_string()),
    };

    Ok((bytes, mime))
}

fn take_screenshot(pdf_path: &Path, page: u32) -> anyhow::Result<(Vec<u8>, String)> {
    let which = std::process::Command::new("which")
        .arg("pdftoppm")
        .output();
    match which {
        Ok(o) if o.status.success() => {}
        _ => anyhow::bail!(
            "pdftoppm not found on PATH. Install poppler: brew install poppler (macOS) or apt install poppler-utils (Linux)"
        ),
    }

    let tmp_dir = tempfile::tempdir()?;
    let prefix = tmp_dir.path().join("page");

    let status = std::process::Command::new("pdftoppm")
        .args([
            "-png",
            "-r",
            "200",
            "-f",
            &page.to_string(),
            "-l",
            &page.to_string(),
            "-singlefile",
        ])
        .arg(pdf_path.as_os_str())
        .arg(prefix.as_os_str())
        .status()?;

    if !status.success() {
        anyhow::bail!("pdftoppm exited with status {status}");
    }

    let png_path = prefix.with_extension("png");
    if !png_path.exists() {
        anyhow::bail!("pdftoppm did not produce expected output file");
    }

    let bytes = std::fs::read(&png_path)?;
    Ok((bytes, "image/png".to_string()))
}

fn write_stream_as_png(
    stream: &pdf_lib_rs::core::objects::PdfRawStream,
    path: &Path,
) -> anyhow::Result<()> {
    use pdf_lib_rs::core::objects::{PdfName, PdfObject};

    let mut decoder = flate2::read::ZlibDecoder::new(&stream.contents[..]);
    let mut raw = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut raw)?;

    let width = match stream.dict.get(&PdfName::of("Width")) {
        Some(PdfObject::Number(n)) => n.as_number() as u32,
        _ => anyhow::bail!("Image missing Width"),
    };
    let height = match stream.dict.get(&PdfName::of("Height")) {
        Some(PdfObject::Number(n)) => n.as_number() as u32,
        _ => anyhow::bail!("Image missing Height"),
    };
    let bpc = match stream.dict.get(&PdfName::of("BitsPerComponent")) {
        Some(PdfObject::Number(n)) => n.as_number() as u8,
        _ => 8,
    };

    let (color_type, components) = match stream.dict.get(&PdfName::of("ColorSpace")) {
        Some(PdfObject::Name(n)) => match n.as_string() {
            "/DeviceRGB" => (png::ColorType::Rgb, 3),
            "/DeviceGray" => (png::ColorType::Grayscale, 1),
            _ => (png::ColorType::Grayscale, 1),
        },
        _ => (png::ColorType::Grayscale, 1),
    };

    let file = std::fs::File::create(path)?;
    let w = &mut std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(color_type);
    let bit_depth = match bpc {
        1 => png::BitDepth::One,
        2 => png::BitDepth::Two,
        4 => png::BitDepth::Four,
        16 => png::BitDepth::Sixteen,
        _ => png::BitDepth::Eight,
    };
    encoder.set_depth(bit_depth);

    let mut writer = encoder.write_header()?;

    let row_bytes = if bpc < 8 {
        (width as usize * components * bpc as usize).div_ceil(8)
    } else {
        width as usize * components * (bpc as usize / 8)
    };
    let expected = row_bytes * height as usize;
    if raw.len() >= expected {
        writer.write_image_data(&raw[..expected])?;
    } else {
        let mut padded = raw;
        padded.resize(expected, 0);
        writer.write_image_data(&padded)?;
    }

    Ok(())
}

fn resolve_to_dict<'a>(
    ctx: &'a pdf_lib_rs::core::context::PdfContext,
    obj: &'a pdf_lib_rs::core::objects::PdfObject,
) -> Option<&'a pdf_lib_rs::core::objects::PdfDict> {
    use pdf_lib_rs::core::objects::PdfObject;
    match obj {
        PdfObject::Dict(d) => Some(d),
        PdfObject::Ref(r) => {
            if let Some(PdfObject::Dict(d)) = ctx.lookup(r) {
                Some(d)
            } else {
                None
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Command-level orchestration (multi-input, multi-page, classifier)
// ---------------------------------------------------------------------------

pub struct CommandArgs {
    pub schema: PathBuf,
    pub inputs: Vec<PathBuf>,
    pub prompt: Option<String>,
    pub model: String,
    pub provider: String,
    pub page: Option<u32>,
    pub pages: Option<String>,
    pub screenshot: bool,
    pub name: Option<String>,
    pub output: Option<PathBuf>,
    pub concurrency: usize,
}

fn is_pdf(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("pdf" | "PDF")
    )
}

pub fn run_command(args: CommandArgs) -> anyhow::Result<()> {
    if args.inputs.is_empty() {
        anyhow::bail!("At least one input file is required");
    }

    let provider_config = resolve_provider(&args.provider)?;
    let is_sqlite = args
        .output
        .as_ref()
        .is_some_and(|p| p.extension().is_some_and(|e| e == "db"));

    let is_md = is_md_schema(&args.schema);

    // Open DB once with WAL mode if SQLite output is requested
    let db = if is_sqlite {
        Some(Mutex::new(sqlite::open_db(args.output.as_ref().unwrap())?))
    } else {
        None
    };

    // Check if markdown has a page_classifier (needs early parse)
    let md_parsed = if is_md {
        let md_text = std::fs::read_to_string(&args.schema)?;
        Some(markdown::parse(&md_text)?)
    } else {
        None
    };

    let has_classifier = md_parsed
        .as_ref()
        .is_some_and(|p| p.frontmatter.page_classifier.is_some());

    // Classifier mode
    if has_classifier {
        let image_path = &args.inputs[0];

        if !is_pdf(image_path) {
            anyhow::bail!("page_classifier requires a PDF input");
        }
        if !is_sqlite {
            anyhow::bail!("page_classifier requires SQLite output (-o <path>.db)");
        }

        let parsed = md_parsed.as_ref().unwrap();
        let classifier_name = parsed.frontmatter.page_classifier.as_deref().unwrap();
        let crop = parsed.frontmatter.classifier_crop.as_ref();

        let page_count = pdf_page_count(image_path)? as u32;
        let page_list: Vec<u32> = if let Some(ref spec) = args.pages {
            pages::parse_page_spec(spec, page_count)?
        } else if let Some(p) = args.page {
            vec![p]
        } else {
            (1..=page_count).collect()
        };

        let total = page_list.len();
        let nc = args.concurrency.min(total).max(1);
        let provider = &args.provider;
        let model = &args.model;
        let screenshot = args.screenshot;
        let db = db.as_ref().unwrap();

        let cr = run_concurrent(nc, &page_list, |page_num, w| {
            w.status(page_num, "rendering page");
            let (image_bytes, _mime) =
                get_image_bytes(image_path, Some(page_num), true)?;

            let classifier_image = if let Some(crop) = crop {
                w.status(page_num, "cropping for classifier");
                crop_image(&image_bytes, crop)?
            } else {
                image_bytes
            };

            w.status(page_num, &format!("classifying via {provider}"));
            let classifier_section =
                markdown::find_section(&parsed.sections, classifier_name)?;
            let classifier_schema =
                parse_schema_content(&classifier_section.schema)?;
            let (classifier_json, _) = call_api(
                &classifier_image,
                "image/png",
                &classifier_schema,
                &classifier_section.prompt,
                model,
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
                get_image_bytes(image_path, Some(page_num), screenshot)?;
            let target_schema =
                parse_schema_content(&target_section.schema)?;
            let (data, timing) = call_api(
                &full_image,
                &full_mime,
                &target_schema,
                &target_section.prompt,
                model,
                &provider_config,
            )?;

            let result = ExtractResult {
                data,
                json_schema: target_schema,
                prompt: target_section.prompt.clone(),
                image_bytes: full_image,
                image_mime: full_mime,
                timing,
            };

            // Insert immediately, holding the lock only for the insert
            {
                let conn = db.lock().unwrap();
                sqlite::insert(
                    &conn,
                    &result,
                    &sqlite::InsertOpts {
                        input_file: image_path,
                        page: Some(page_num),
                        page_count: Some(page_count),
                        model,
                        classifier_data: Some(&classifier_json),
                    },
                )?;
            }

            Ok(())
        }, |page_num, e| {
            eprintln!("  page {page_num}: {e:#}");
            let conn = db.lock().unwrap();
            let _ = sqlite::insert_error(&conn, &sqlite::ErrorOpts {
                input_file: image_path,
                page: Some(page_num),
                model,
                error: &format!("{e:#}"),
            });
        });

        let msg = if cr.error_count > 0 {
            format!(
                "Classified and extracted {} pages ({} errors) into {}",
                total - cr.error_count, cr.error_count, args.output.as_ref().unwrap().display()
            )
        } else {
            format!(
                "Classified and extracted {total} pages into {}",
                args.output.as_ref().unwrap().display()
            )
        };
        cr.prog.finish(&msg);
        return Ok(());
    }

    // --- Non-classifier flow ---

    // For a single input, use the simple path
    if args.inputs.len() == 1 {
        let input = &args.inputs[0];
        let is_clip = is_clipboard(input);
        let input_is_pdf = !is_clip && is_pdf(input);

        let page_list: Vec<u32> = if let Some(ref spec) = args.pages {
            if !input_is_pdf {
                anyhow::bail!("--pages is only valid for PDF files");
            }
            let count = pdf_page_count(input)? as u32;
            pages::parse_page_spec(spec, count)?
        } else if let Some(p) = args.page {
            vec![p]
        } else if input_is_pdf && is_sqlite {
            let count = pdf_page_count(input)? as u32;
            (1..=count).collect()
        } else {
            vec![]
        };

        if page_list.len() > 1 && !is_sqlite {
            anyhow::bail!("Multiple pages require SQLite output (-o <path>.db)");
        }

        // Single-page path
        if page_list.len() <= 1 {
            let effective_page = page_list.first().copied().or(args.page);
            let ea = ExtractArgs {
                schema: &args.schema,
                input,
                prompt: args.prompt.as_deref(),
                model: &args.model,
                provider: &provider_config,
                page: effective_page,
                screenshot: args.screenshot,
                name: args.name.as_deref(),
            };
            let result = run(&ea)?;

            eprintln!(
                "Extracted in {}ms ({} → {})",
                result.timing.elapsed_ms, result.timing.started_at, result.timing.finished_at,
            );

            if let Some(ref db) = db {
                let conn = db.lock().unwrap();
                let page_count = if input_is_pdf {
                    Some(pdf_page_count(input)? as u32)
                } else {
                    None
                };
                sqlite::insert(
                    &conn,
                    &result,
                    &sqlite::InsertOpts {
                        input_file: input,
                        page: effective_page,
                        page_count,
                        model: &args.model,
                        classifier_data: None,
                    },
                )?;
                eprintln!("Inserted into {}", args.output.as_ref().unwrap().display());
            } else if let Some(path) = &args.output {
                std::fs::write(path, &result.data)?;
                eprintln!("Wrote {}", path.display());
            } else {
                println!("{}", result.data);
            }

            return Ok(());
        }

        // Multi-page SQLite path (concurrent)
        let total = page_list.len();
        let nc = args.concurrency.min(total).max(1);

        let json_schema: serde_json::Value = if is_md {
            let md_text = std::fs::read_to_string(&args.schema)?;
            let parsed = markdown::parse(&md_text)?;
            let section =
                markdown::resolve_section(&parsed.sections, args.name.as_deref())?;
            parse_schema_content(&section.schema)?
        } else {
            serde_json::from_str(&js_runner::run(&args.schema)?)?
        };

        let prompt_str = args
            .prompt
            .as_deref()
            .unwrap_or("Extract the structured data from this image.");
        let model = &args.model;
        let provider = &args.provider;
        let screenshot = args.screenshot;
        let db = db.as_ref().unwrap();
        let page_count = if input_is_pdf {
            Some(pdf_page_count(input)? as u32)
        } else {
            None
        };

        let cr = run_concurrent(nc, &page_list, |page_num, w| {
            w.status(page_num, "extracting image");
            let (image_bytes, mime) =
                get_image_bytes(input, Some(page_num), screenshot)?;

            w.status(page_num, &format!("calling {provider}"));
            let (data, timing) =
                call_api(&image_bytes, &mime, &json_schema, prompt_str, model, &provider_config)?;

            let result = ExtractResult {
                data,
                json_schema: json_schema.clone(),
                prompt: prompt_str.to_string(),
                image_bytes,
                image_mime: mime,
                timing,
            };

            {
                let conn = db.lock().unwrap();
                sqlite::insert(
                    &conn,
                    &result,
                    &sqlite::InsertOpts {
                        input_file: input,
                        page: Some(page_num),
                        page_count,
                        model,
                        classifier_data: None,
                    },
                )?;
            }

            Ok(())
        }, |page_num, e| {
            eprintln!("  page {page_num}: {e:#}");
            let conn = db.lock().unwrap();
            let _ = sqlite::insert_error(&conn, &sqlite::ErrorOpts {
                input_file: input,
                page: Some(page_num),
                model,
                error: &format!("{e:#}"),
            });
        });

        let msg = if cr.error_count > 0 {
            format!(
                "Extracted {} pages ({} errors) into {}",
                total - cr.error_count, cr.error_count, args.output.as_ref().unwrap().display()
            )
        } else {
            format!(
                "Extracted {total} pages into {}",
                args.output.as_ref().unwrap().display()
            )
        };
        cr.prog.finish(&msg);
        return Ok(());
    }

    // --- Multiple inputs ---
    if !is_sqlite {
        anyhow::bail!("Multiple inputs require SQLite output (-o <path>.db)");
    }
    let db = db.as_ref().unwrap();

    for input in &args.inputs {
        let is_clip = is_clipboard(input);
        let input_is_pdf = !is_clip && is_pdf(input);

        let page_list: Vec<u32> = if let Some(ref spec) = args.pages {
            if !input_is_pdf {
                anyhow::bail!("--pages is only valid for PDF files");
            }
            let count = pdf_page_count(input)? as u32;
            pages::parse_page_spec(spec, count)?
        } else if let Some(p) = args.page {
            vec![p]
        } else if input_is_pdf {
            let count = pdf_page_count(input)? as u32;
            (1..=count).collect()
        } else {
            vec![0] // sentinel for non-PDF single image
        };

        let json_schema: serde_json::Value = if is_md {
            let md_text = std::fs::read_to_string(&args.schema)?;
            let parsed = markdown::parse(&md_text)?;
            let section =
                markdown::resolve_section(&parsed.sections, args.name.as_deref())?;
            parse_schema_content(&section.schema)?
        } else {
            serde_json::from_str(&js_runner::run(&args.schema)?)?
        };

        let prompt_str = args
            .prompt
            .as_deref()
            .unwrap_or("Extract the structured data from this image.");
        let model = &args.model;
        let provider = &args.provider;
        let screenshot = args.screenshot;
        let page_count = if input_is_pdf {
            Some(pdf_page_count(input)? as u32)
        } else {
            None
        };

        let total = page_list.len();
        let nc = args.concurrency.min(total).max(1);

        let cr = run_concurrent(nc, &page_list, |page_num, w| {
            let effective_page = if page_num == 0 { None } else { Some(page_num) };
            w.status(page_num, "extracting image");
            let (image_bytes, mime) =
                get_image_bytes(input, effective_page, screenshot)?;

            w.status(page_num, &format!("calling {provider}"));
            let (data, timing) =
                call_api(&image_bytes, &mime, &json_schema, prompt_str, model, &provider_config)?;

            let result = ExtractResult {
                data,
                json_schema: json_schema.clone(),
                prompt: prompt_str.to_string(),
                image_bytes,
                image_mime: mime,
                timing,
            };

            {
                let conn = db.lock().unwrap();
                sqlite::insert(
                    &conn,
                    &result,
                    &sqlite::InsertOpts {
                        input_file: input,
                        page: effective_page,
                        page_count,
                        model,
                        classifier_data: None,
                    },
                )?;
            }

            Ok(())
        }, |page_num, e| {
            let effective_page = if page_num == 0 { None } else { Some(page_num) };
            eprintln!("  {}: {e:#}", input.display());
            let conn = db.lock().unwrap();
            let _ = sqlite::insert_error(&conn, &sqlite::ErrorOpts {
                input_file: input,
                page: effective_page,
                model,
                error: &format!("{e:#}"),
            });
        });

        let msg = if cr.error_count > 0 {
            format!(
                "Extracted {} from {} ({} errors) into {}",
                total - cr.error_count, input.display(), cr.error_count,
                args.output.as_ref().unwrap().display()
            )
        } else {
            format!(
                "Extracted {} from {} into {}",
                total, input.display(), args.output.as_ref().unwrap().display()
            )
        };
        cr.prog.finish(&msg);
    }

    Ok(())
}

struct ConcurrentResult {
    prog: Progress,
    error_count: usize,
}

/// Process pages concurrently with up to `nc` worker threads.
/// The callback should handle its own persistence (e.g. SQLite inserts).
/// On error, `on_error` is called and work continues with the remaining pages.
fn run_concurrent<F, E>(
    nc: usize,
    page_list: &[u32],
    process: F,
    on_error: E,
) -> ConcurrentResult
where
    F: Fn(u32, &Worker<'_>) -> anyhow::Result<()> + Sync,
    E: Fn(u32, &anyhow::Error) + Sync,
{
    let total = page_list.len();
    let prog = Progress::new(total, nc);
    let work = Mutex::new(page_list.iter().copied());
    let error_count = std::sync::atomic::AtomicUsize::new(0);

    std::thread::scope(|s| {
        for worker_idx in 0..nc {
            let work = &work;
            let prog = &prog;
            let process = &process;
            let on_error = &on_error;
            let error_count = &error_count;

            s.spawn(move || {
                let w = prog.worker(worker_idx);
                loop {
                    let page_num = {
                        let mut iter = work.lock().unwrap();
                        iter.next()
                    };
                    let Some(page_num) = page_num else {
                        break;
                    };

                    match process(page_num, &w) {
                        Ok(()) => {
                            w.complete_page();
                        }
                        Err(e) => {
                            on_error(page_num, &e);
                            error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            w.complete_page();
                        }
                    }
                }
            });
        }
    });

    ConcurrentResult {
        prog,
        error_count: error_count.load(std::sync::atomic::Ordering::Relaxed),
    }
}
