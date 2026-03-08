use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "schema-extract")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Convert a Zod schema file to JSON Schema
    Schema {
        /// Path to a .js file with `export default z.object(...)`
        file: PathBuf,
    },
    /// Validate a markdown recipe file (schemas, sections, frontmatter)
    Check {
        /// Path to the .md recipe file
        file: PathBuf,
    },
    /// Extract structured data from an image or PDF using a Zod schema via an LLM provider
    ///
    /// Two modes:
    ///   1. extract <input> <schema.js> --prompt "..."
    ///   2. extract <input.md>  (markdown with ```schema blocks)
    Extract {
        /// Path to the image, PDF, or markdown file
        input: PathBuf,
        /// Path to a .js Zod schema file (not needed when input is .md)
        schema: Option<PathBuf>,
        /// Prompt telling the model what to extract (not needed when input is .md)
        #[arg(long)]
        prompt: Option<String>,
        /// Model identifier (format depends on provider)
        #[arg(long, default_value = "google/gemini-3-flash-preview")]
        model: String,
        /// Inference provider: "openrouter", "llamabarn", or a custom OpenAI-compatible base URL
        #[arg(short = 'P', long, default_value = "openrouter")]
        provider: String,
        /// Single page number (1-based); conflicts with --pages
        #[arg(long, conflicts_with = "pages")]
        page: Option<u32>,
        /// Page selection (e.g. "1-3,5,21"); defaults to all pages in SQLite mode
        #[arg(short, long, conflicts_with = "page")]
        pages: Option<String>,
        /// Take a screenshot of the PDF page instead of extracting an embedded image
        #[arg(long)]
        screenshot: bool,
        /// Section name to use when markdown has multiple sections
        #[arg(long)]
        name: Option<String>,
        /// Output path; if it ends with .db, writes to SQLite instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Number of concurrent pages to process (default 8)
        #[arg(short = 'n', long = "nc", default_value = "8")]
        concurrency: usize,
    },
}
