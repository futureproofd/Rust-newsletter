use std::net::TcpListener;

use rust_newsletter::configuration::get_configuration;
use rust_newsletter::startup::run;
use rust_newsletter::telemetry::{get_subscriber, init_subscriber};
use sqlx::PgPool;

use secrecy::ExposeSecret;

// run: `cargo +nightly expand --bin rust-newsletter-bin` (use nightly compiler for the 'expand' cmd only) to view macro expansion
#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Old logger:
    // `init` does call `set_logger`, so this is all we need to do.
    // We are falling back to printing all logs at info-level or above
    // if the RUST_LOG environment variable has not been set.
    // env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let subscriber = get_subscriber("rust_newsletter".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    // Panic if we cannot read the config
    let configuration = get_configuration().expect("Failed to read configuration.");
    // get a connection pool for multiple connections
    let connection = PgPool::connect(&configuration.database.connection_string().expose_secret())
        .await
        .expect("Failed to connect to Postgres.");
    let address = format!("127.0.0.1:{}", configuration.application_port);
    // Bubble up the io::Error if we failed (?) to bind the address
    // Otherwise call .await on our Server
    let listener = TcpListener::bind(address)?;
    run(listener, connection)?.await
}
