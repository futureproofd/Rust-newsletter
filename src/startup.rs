use crate::configuration::{DatabaseSettings, Settings};
use actix_web::{dev::Server, web, App, HttpServer};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

use crate::email_client::EmailClient;
use crate::routes::{health_check, subscribe};

// a new type to hold the newly built Actix server and it's port
pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    // the build function is now a constructor for the Application type
    pub async fn build(configuration: Settings) -> Result<Self, std::io::Error> {
        // get a connection pool for multiple connections
        let connection = get_connection_pool(&configuration.database);

        // build an email client using configuration
        let sender_email = configuration
            .email_client
            .sender()
            .expect("Invalid sender address");

        let timeout = configuration.email_client.timeout();

        let email_client = EmailClient::new(
            configuration.email_client.base_url,
            configuration.email_client.authorization_token,
            sender_email,
            timeout,
        );

        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );
        // Bubble up the io::Error if we failed (?) to bind the address
        // Otherwise call .await on our Server
        let listener = TcpListener::bind(address)?;

        let port = listener.local_addr().unwrap().port();
        let server = run(listener, connection, email_client)?;

        // we "save" the bound port in one of Application's fields
        Ok(Self { port, server })
    }

    pub fn port(&self) -> u16 {
        self.port
    }
    // A more expressive name that makes it clear that
    // this function only returns when the application is stopped.
    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

// We need to mark `run` as public.
// It is no longer a binary entrypoint, therefore we can mark it as async // without having to use any proc-macro incantation.

// Notice the different signature!
// We return `Server` on the happy path and we dropped the `async` keyword // We have no .await call, so it is not needed anymore.
pub fn run(
    listener: TcpListener,
    db_pool: PgPool,
    email_client: EmailClient,
) -> Result<Server, std::io::Error> {
    // Instead of getting a raw copy of a PgConnection, will get a (Arc) pointer to one
    let db_pool = web::Data::new(db_pool);
    let email_client = web::Data::new(email_client);

    let server = HttpServer::new(move || {
        App::new()
            // Middlewares are added using the `wrap` method on `App`
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            // Register the connection as part of the application state,
            // and get a pointer copy and attach it to the application state
            .app_data(db_pool.clone())
            .app_data(email_client.clone())
    })
    .listen(listener)?
    .run();
    //no more .await
    Ok(server)
}

pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy_with(configuration.with_db())
}
