use rust_newsletter::run;

// run: `cargo +nightly expand` (use nightly compiler for the 'expand' cmd only) to view macro expansion
#[tokio::main]
async fn main() -> std::io::Result<()> {
    run().await
}
