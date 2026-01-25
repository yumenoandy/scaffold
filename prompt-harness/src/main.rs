fn main() {
    println!("prompt-harness: Simple LLM API clients");
    println!();
    println!("Available providers:");
    println!("  - AnthropicProvider (LLM completion)");
    println!("  - OpenAIProvider (LLM completion)");
    println!("  - GeminiProvider (LLM completion)");
    println!("  - MistralOcrProvider (Document OCR)");
    println!();
    println!("Run examples:");
    println!("  cargo run --example mistral_ocr -- document.pdf");
}
