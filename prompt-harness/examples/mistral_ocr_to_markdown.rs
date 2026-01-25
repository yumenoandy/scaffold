//! Example: Mistral OCR to Markdown
//!
//! Runs OCR on a document and saves the results as a markdown file with extracted
//! images written as separate files alongside it.
//!
//! # Usage
//!
//! ```bash
//! export MISTRAL_API_KEY="your-api-key"
//!
//! # Basic usage - creates output.md and images/ in current directory
//! cargo run --example mistral_ocr_to_markdown -- document.pdf
//!
//! # Specify output directory
//! cargo run --example mistral_ocr_to_markdown -- document.pdf -o output/
//!
//! # Process specific pages only
//! cargo run --example mistral_ocr_to_markdown -- big.pdf -p 0-9 -o ch1/
//!
//! # Replay a saved raw API response (no API key needed)
//! cargo run --example mistral_ocr_to_markdown -- --replay output/document.raw.json -o output/
//! ```

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use prompt_harness::mistral::{MistralOcrProvider, Request, TableFormat};

fn main() {
    let args: Vec<String> = env::args().collect();
    let config = parse_args(&args);

    let (response, stem) = if let Some(replay_path) = &config.replay {
        // Replay mode: parse a saved raw API response
        let body = match fs::read_to_string(replay_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Failed to read replay file '{}': {}", replay_path, e);
                process::exit(1);
            }
        };
        eprintln!("Replaying: {}", replay_path);
        let response = match MistralOcrProvider::parse_raw_response(&body) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to parse replay file: {}", e);
                process::exit(1);
            }
        };
        let stem = Path::new(replay_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix(".raw"))
            .unwrap_or("output")
            .to_string();
        (response, stem)
    } else {
        // Live mode: call the API
        let file_path = config.file_path.as_deref().unwrap_or_else(|| {
            eprintln!("Error: No file specified");
            print_usage();
            process::exit(1);
        });

        let mime_type = mime_type_from_path(file_path);

        let Ok(document) = fs::read(file_path) else {
            eprintln!("Failed to read file '{}'", file_path);
            process::exit(1);
        };

        eprintln!(
            "Processing: {} ({}, {} bytes)",
            file_path, mime_type, document.len()
        );

        let Ok(provider) = MistralOcrProvider::latest() else {
            eprintln!("Failed to create provider");
            eprintln!();
            eprintln!("Make sure MISTRAL_API_KEY is set:");
            eprintln!("  export MISTRAL_API_KEY=\"your-api-key\"");
            process::exit(1);
        };

        let request = Request {
            document: &document,
            mime_type,
            pages: config.pages.as_deref(),
            include_images: true,
            table_format: Some(TableFormat::Markdown),
            extract_headers: true,
            extract_footers: true,
            ..Default::default()
        };

        let result = match provider.process_raw(&request) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("OCR failed: {}", e);
                process::exit(1);
            }
        };

        let stem = Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string();

        // Save raw API response
        let output_dir = &config.output_dir;
        if let Err(e) = fs::create_dir_all(output_dir) {
            eprintln!("Failed to create directory '{}': {}", output_dir.display(), e);
            process::exit(1);
        }
        let raw_path = output_dir.join(format!("{}.raw.json", stem));
        if let Err(e) = fs::write(&raw_path, &result.raw_json) {
            eprintln!("Warning: failed to save raw response: {}", e);
        } else {
            eprintln!("Raw response saved: {}", raw_path.display());
        }

        (result.response, stem)
    };

    // Create output directory and images subdirectory
    let output_dir = &config.output_dir;
    if let Err(e) = fs::create_dir_all(output_dir) {
        eprintln!("Failed to create directory '{}': {}", output_dir.display(), e);
        process::exit(1);
    }

    // Save processed response as JSON
    let json_path = output_dir.join(format!("{}.json", stem));
    match serde_json::to_string_pretty(&response) {
        Ok(json) => {
            if let Err(e) = fs::write(&json_path, &json) {
                eprintln!("Warning: failed to write JSON: {}", e);
            } else {
                eprintln!("Response saved: {}", json_path.display());
            }
        }
        Err(e) => eprintln!("Warning: failed to serialize response: {}", e),
    }

    let images_dir = output_dir.join("images");
    if let Err(e) = fs::create_dir_all(&images_dir) {
        eprintln!("Failed to create directory '{}': {}", images_dir.display(), e);
        process::exit(1);
    }

    // Build global table map (0-based numbering across all pages)
    let mut table_map: Vec<(&str, usize)> = Vec::new();
    for page in &response.pages {
        for table in &page.tables {
            table_map.push((&table.content, table_map.len()));
        }
    }

    let mut markdown = String::new();
    let mut images_saved = 0u32;

    for page in &response.pages {
        if page.index > 0 {
            markdown.push_str("\n---\n\n");
        }

        let mut page_md = page.markdown.clone();

        // Replace table references with actual table content
        for (content, n) in &table_map {
            let patterns = [
                format!("![tbl-{}.md](tbl-{}.md)", n, n),
                format!("[tbl-{}.md](tbl-{}.md)", n, n),
            ];
            for pattern in &patterns {
                if page_md.contains(pattern) {
                    page_md = page_md.replace(pattern, content);
                    break;
                }
            }
        }

        for img in &page.images {
            if let Some(b64) = &img.image_base64 {
                let (ext, raw_b64) = parse_image_data(b64);
                let img_name = if Path::new(&img.id).extension().is_some() {
                    img.id.clone()
                } else {
                    format!("{}.{}", img.id, ext)
                };
                let filename = format!("{}_p{}_{}", stem, page.index, img_name);
                let img_path = images_dir.join(&filename);

                match base64_decode(raw_b64) {
                    Ok(bytes) => {
                        if let Err(e) = fs::write(&img_path, &bytes) {
                            eprintln!("Warning: failed to write {}: {}", img_path.display(), e);
                            continue;
                        }
                        images_saved += 1;

                        let local_ref = format!("![{}](images/{})", img.id, filename);
                        let api_pattern = format!("![{}](", img.id);
                        if let Some(start) = page_md.find(&api_pattern) {
                            if let Some(end) = page_md[start..].find(')') {
                                page_md.replace_range(start..start + end + 1, &local_ref);
                            }
                        } else {
                            page_md.push_str(&format!("\n\n{}\n", local_ref));
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: failed to decode image {}: {}", img.id, e);
                    }
                }
            }
        }

        markdown.push_str(&page_md);
        markdown.push('\n');
    }

    let md_path = output_dir.join(format!("{}.md", stem));
    if let Err(e) = fs::write(&md_path, &markdown) {
        eprintln!("Failed to write markdown file: {}", e);
        process::exit(1);
    }

    eprintln!("Saved: {}", md_path.display());
    eprintln!("Pages: {}", response.pages_processed);
    eprintln!("Images saved: {}", images_saved);
}

/// Strip data URL prefix if present, returning (extension, raw_base64).
fn parse_image_data(data: &str) -> (&str, &str) {
    if let Some(rest) = data.strip_prefix("data:") {
        if let Some((header, b64)) = rest.split_once(',') {
            let ext = header
                .strip_prefix("image/")
                .and_then(|s| s.split(';').next())
                .unwrap_or("png");
            return (ext, b64);
        }
    }
    ("png", data)
}

fn base64_decode(data: &str) -> std::result::Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| format!("base64 decode error: {}", e))
}

fn mime_type_from_path(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some(ext) => {
            eprintln!("Unsupported file extension: {}", ext);
            process::exit(1);
        }
        None => {
            eprintln!("Could not determine file type from extension");
            process::exit(1);
        }
    }
}

struct Config {
    file_path: Option<String>,
    output_dir: PathBuf,
    pages: Option<Vec<u32>>,
    replay: Option<String>,
}

fn parse_args(args: &[String]) -> Config {
    let mut file_path = None;
    let mut output_dir = None;
    let mut pages = None;
    let mut replay = None;
    let mut iter = args.iter().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                output_dir = iter.next().map(PathBuf::from);
                if output_dir.is_none() {
                    eprintln!("Error: --output requires a directory path");
                    process::exit(1);
                }
            }
            "-p" | "--pages" => {
                let Some(spec) = iter.next() else {
                    eprintln!("Error: --pages requires a range (e.g. 0-9)");
                    process::exit(1);
                };
                pages = Some(parse_page_range(spec));
            }
            "--replay" => {
                replay = iter.next().map(|s| s.clone());
                if replay.is_none() {
                    eprintln!("Error: --replay requires a file path");
                    process::exit(1);
                }
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            _ if arg.starts_with('-') => {
                eprintln!("Unknown option: {}", arg);
                print_usage();
                process::exit(1);
            }
            _ => file_path = Some(arg.clone()),
        }
    }

    if replay.is_none() && file_path.is_none() {
        eprintln!("Error: No file specified");
        print_usage();
        process::exit(1);
    }

    let output_dir = output_dir.unwrap_or_else(|| PathBuf::from("."));

    Config { file_path, output_dir, pages, replay }
}

/// Parse a page range like "0-9", "5", or "0,2,4,6".
fn parse_page_range(spec: &str) -> Vec<u32> {
    if let Some((start, end)) = spec.split_once('-') {
        let start: u32 = start.parse().unwrap_or_else(|_| {
            eprintln!("Invalid page range: {}", spec);
            process::exit(1);
        });
        let end: u32 = end.parse().unwrap_or_else(|_| {
            eprintln!("Invalid page range: {}", spec);
            process::exit(1);
        });
        (start..=end).collect()
    } else {
        spec.split(',')
            .map(|s| {
                s.trim().parse::<u32>().unwrap_or_else(|_| {
                    eprintln!("Invalid page number: {}", s);
                    process::exit(1);
                })
            })
            .collect()
    }
}

fn print_usage() {
    eprintln!();
    eprintln!("Usage: cargo run --example mistral_ocr_to_markdown -- [OPTIONS] <FILE>");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  <FILE>                PDF or image file to process");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -o, --output <DIR>    Output directory (default: current directory)");
    eprintln!("  -p, --pages <RANGE>   Page range to process (e.g. 0-9, 0,2,4)");
    eprintln!("  --replay <PATH>       Replay a saved raw API response (no API key needed)");
    eprintln!("  -h, --help            Print this help message");
    eprintln!();
    eprintln!("Output:");
    eprintln!("  <DIR>/<stem>.raw.json Raw API response (for replay)");
    eprintln!("  <DIR>/<stem>.json     Processed response");
    eprintln!("  <DIR>/<stem>.md       Markdown file with OCR results");
    eprintln!("  <DIR>/images/         Extracted images");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  MISTRAL_API_KEY       Your Mistral API key (required unless using --replay)");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  cargo run --example mistral_ocr_to_markdown -- document.pdf");
    eprintln!("  cargo run --example mistral_ocr_to_markdown -- document.pdf -o results/");
    eprintln!("  cargo run --example mistral_ocr_to_markdown -- big.pdf -p 0-9 -o ch1/");
    eprintln!("  cargo run --example mistral_ocr_to_markdown -- --replay results/doc.raw.json -o results/");
}
