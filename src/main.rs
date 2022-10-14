use std::net::TcpListener;

use rust_newsletter::configuration::get_configuration;
use rust_newsletter::startup::run;
use sqlx::PgPool;

use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::BunyanFormattingLayer;
use tracing_log::LogTracer;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};

// run: `cargo +nightly expand --bin rust-newsletter-bin` (use nightly compiler for the 'expand' cmd only) to view macro expansion
#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Old logger:
    // `init` does call `set_logger`, so this is all we need to do.
    // We are falling back to printing all logs at info-level or above
    // if the RUST_LOG environment variable has not been set.
    // env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // Redirect all `log`'s events to our subscriber
    LogTracer::init().expect("Failed to set logger");

    // new, tracing implementation:
    // We are falling back to printing all spans at info-level or above // if the RUST_LOG environment variable has not been set.
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let formatting_layer = BunyanFormattingLayer::new(
        "rust_newsletter".into(),
        // Output the formatted spans to stdout.
        std::io::stdout,
    );

    // The `with` method is provided by `SubscriberExt`, an extension trait for `Subscriber` exposed by `tracing_subscriber`
    let subscriber = Registry::default().with(env_filter).with(formatting_layer);
    set_global_default(subscriber).expect("failed to set the subscriber");

    // Panic if we cannot read the config
    let configuration = get_configuration().expect("Failed to read configuration.");
    // get a connection pool for multiple connections
    let connection = PgPool::connect(&configuration.database.connection_string())
        .await
        .expect("Failed to connect to Postgres.");
    let address = format!("127.0.0.1:{}", configuration.application_port);
    // Bubble up the io::Error if we failed (?) to bind the address
    // Otherwise call .await on our Server
    let listener = TcpListener::bind(address)?;
    run(listener, connection)?.await
}
