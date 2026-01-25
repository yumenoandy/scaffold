//! Example: Mistral OCR
//!
//! Demonstrates how to use the Mistral OCR provider to extract text from documents.
//!
//! # Usage
//!
//! ```bash
//! # Set your API key
//! export MISTRAL_API_KEY="your-api-key"
//!
//! # Run on a PDF
//! cargo run --example mistral_ocr -- document.pdf
//!
//! # Run on an image
//! cargo run --example mistral_ocr -- screenshot.png
//!
//! # With verbose output (show images and tables)
//! cargo run --example mistral_ocr -- --verbose document.pdf
//!
//! # Replay a saved raw API response (no API key needed)
//! cargo run --example mistral_ocr -- --replay document.raw.json
//! ```

use std::env;
use std::fs;
use std::path::Path;
use std::process;

use prompt_harness::mistral::{MistralOcrProvider, Request, Response, TableFormat};

fn main() {
    let args: Vec<String> = env::args().collect();
    let config = parse_args(&args);

    let response = if let Some(replay_path) = &config.replay {
        // Replay mode: parse a saved raw API response
        let body = match fs::read_to_string(replay_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Failed to read replay file '{}': {}", replay_path, e);
                process::exit(1);
            }
        };
        eprintln!("Replaying: {}", replay_path);
        match MistralOcrProvider::parse_raw_response(&body) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to parse replay file: {}", e);
                process::exit(1);
            }
        }
    } else {
        // Live mode: call the API and save the raw response
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
            include_images: config.verbose,
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

        // Save raw API response
        let stem = Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        let raw_path = format!("{}.raw.json", stem);
        if let Err(e) = fs::write(&raw_path, &result.raw_json) {
            eprintln!("Warning: failed to save raw response: {}", e);
        } else {
            eprintln!("Raw response saved: {}", raw_path);
        }

        result.response
    };

    print_response(&response, config.verbose);
}

fn print_response(response: &Response, verbose: bool) {
    println!("Model: {}", response.model);
    println!("Pages processed: {}", response.pages_processed);
    if let Some(size) = response.doc_size_bytes {
        println!("Document size: {} bytes", size);
    }
    println!();
    println!("{}", "=".repeat(80));

    for page in &response.pages {
        println!();
        println!("PAGE {} ", page.index);
        println!("{}", "-".repeat(40));

        // Dimensions
        if let Some(dim) = &page.dimensions {
            println!(
                "Dimensions: {}x{} px{}",
                dim.width,
                dim.height,
                dim.dpi.map(|d| format!(" @ {} dpi", d)).unwrap_or_default()
            );
        }

        // Header
        if let Some(header) = &page.header {
            println!("Header: {}", header);
        }

        // Footer
        if let Some(footer) = &page.footer {
            println!("Footer: {}", footer);
        }

        // Content
        println!();
        println!("CONTENT:");
        println!("{}", page.markdown);

        // Images
        if !page.images.is_empty() {
            println!();
            println!("IMAGES ({}):", page.images.len());
            for img in &page.images {
                println!(
                    "  - {} @ ({}, {}) to ({}, {}) [{}x{}]",
                    img.id,
                    img.bbox.top_left_x,
                    img.bbox.top_left_y,
                    img.bbox.bottom_right_x,
                    img.bbox.bottom_right_y,
                    img.bbox.width(),
                    img.bbox.height()
                );
                if verbose {
                    if let Some(b64) = &img.image_base64 {
                        println!(
                            "    base64: {}... ({} chars)",
                            &b64[..b64.len().min(50)],
                            b64.len()
                        );
                    }
                }
            }
        }

        // Tables
        if !page.tables.is_empty() {
            println!();
            println!("TABLES ({}):", page.tables.len());
            for (i, table) in page.tables.iter().enumerate() {
                println!("  Table {} ({}):", i + 1, table.format);
                if verbose {
                    for line in table.content.lines().take(10) {
                        println!("    {}", line);
                    }
                    if table.content.lines().count() > 10 {
                        println!(
                            "    ... ({} more lines)",
                            table.content.lines().count() - 10
                        );
                    }
                }
            }
        }

        // Hyperlinks
        if !page.hyperlinks.is_empty() {
            println!();
            println!("HYPERLINKS ({}):", page.hyperlinks.len());
            for link in &page.hyperlinks {
                if link.text == link.url {
                    println!("  - {}", link.url);
                } else {
                    println!("  - {} -> {}", link.text, link.url);
                }
            }
        }

        println!();
        println!("{}", "=".repeat(80));
    }

    println!();
    println!("Done!");
}

struct Config {
    verbose: bool,
    file_path: Option<String>,
    replay: Option<String>,
}

fn parse_args(args: &[String]) -> Config {
    let mut verbose = false;
    let mut file_path = None;
    let mut replay = None;
    let mut iter = args.iter().skip(1);

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-v" | "--verbose" => verbose = true,
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

    Config { verbose, file_path, replay }
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

fn print_usage() {
    eprintln!();
    eprintln!("Usage: cargo run --example mistral_ocr -- [OPTIONS] <FILE>");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  <FILE>           PDF or image file to process");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -v, --verbose    Show detailed output (images, tables)");
    eprintln!("  --replay <PATH>  Replay a saved raw API response (no API key needed)");
    eprintln!("  -h, --help       Print this help message");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  MISTRAL_API_KEY  Your Mistral API key (required unless using --replay)");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  cargo run --example mistral_ocr -- document.pdf");
    eprintln!("  cargo run --example mistral_ocr -- --verbose image.png");
    eprintln!("  cargo run --example mistral_ocr -- --replay document.raw.json");
}
