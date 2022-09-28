use std::net::TcpListener;

use rust_newsletter::configuration::get_configuration;
use rust_newsletter::startup::run;
use sqlx::PgPool;

// run: `cargo +nightly expand --bin rust-newsletter-bin` (use nightly compiler for the 'expand' cmd only) to view macro expansion
#[tokio::main]
async fn main() -> std::io::Result<()> {
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
