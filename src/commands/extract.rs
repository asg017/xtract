use base64::Engine;
use std::path::Path;
use std::time::Instant;

use crate::js_runner;
use crate::markdown;
use crate::markdown::CropRegion;

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
    pub input: &'a Path,
    pub schema: Option<&'a Path>,
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

pub fn run(args: &ExtractArgs) -> anyhow::Result<ExtractResult> {
    let is_md = matches!(
        args.input.extension().and_then(|e| e.to_str()),
        Some("md" | "markdown")
    );

    if is_md {
        return run_markdown(args);
    }

    // Non-markdown mode: schema is required
    let schema_path = args
        .schema
        .ok_or_else(|| anyhow::anyhow!("Schema file is required (or use a .md file as input)"))?;

    let prompt = args
        .prompt
        .unwrap_or("Extract the structured data from this image.")
        .to_string();

    let (image_bytes, mime) = get_image_bytes(args.input, args.page, args.screenshot)?;
    let json_schema: serde_json::Value = serde_json::from_str(&js_runner::run(schema_path)?)?;

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
    let md_text = std::fs::read_to_string(args.input)?;
    let parsed = markdown::parse(&md_text)?;
    let section = markdown::resolve_section(&parsed.sections, args.name)?;

    // The schema content from the ```schema block could be Zod JS or raw JSON Schema
    let json_schema = parse_schema_content(&section.schema)?;

    // We need an image/PDF input — check the second positional arg
    let image_path = args.schema.ok_or_else(|| {
        anyhow::anyhow!(
            "When using a markdown file, provide the image/PDF path as the second argument:\n  \
             schema-extract extract recipe.md photo.jpg"
        )
    })?;

    let (image_bytes, mime) = get_image_bytes(image_path, args.page, args.screenshot)?;
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

pub fn get_image_bytes(
    input: &Path,
    page: Option<u32>,
    screenshot: bool,
) -> anyhow::Result<(Vec<u8>, String)> {
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
    let mut req = ureq::post(&url)
        .header("Content-Type", "application/json");
    if let Some(ref api_key) = provider.api_key {
        req = req.header("Authorization", &format!("Bearer {api_key}"));
    }

    let started_at = now_iso();
    let start = Instant::now();

    let response: serde_json::Value = req
        .send_json(&body)?
        .body_mut()
        .read_json()?;

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
