use std::net::TcpListener;

use rust_newsletter::configuration::get_configuration;
use rust_newsletter::email_client::EmailClient;
use rust_newsletter::startup::run;
use rust_newsletter::telemetry::{get_subscriber, init_subscriber};
use sqlx::postgres::PgPoolOptions;

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
    let connection = PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy_with(configuration.database.with_db());

    // build an email client using configuration
    let sender_email = configuration
        .email_client
        .sender()
        .expect("Invalid sender address");
    let email_client = EmailClient::new(
        configuration.email_client.base_url,
        configuration.email_client.authorization_token,
        sender_email,
    );

    let address = format!(
        "{}:{}",
        configuration.application.host, configuration.application.port
    );
    // Bubble up the io::Error if we failed (?) to bind the address
    // Otherwise call .await on our Server
    let listener = TcpListener::bind(address)?;
    run(listener, connection, email_client)?.await
}
