use actix_web::{dev::Server, web, App, HttpServer};
use sqlx::PgPool;
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

use crate::routes::{health_check, subscribe};
// We need to mark `run` as public.
// It is no longer a binary entrypoint, therefore we can mark it as async // without having to use any proc-macro incantation.

// Notice the different signature!
// We return `Server` on the happy path and we dropped the `async` keyword // We have no .await call, so it is not needed anymore.
pub fn run(listener: TcpListener, db_pool: PgPool) -> Result<Server, std::io::Error> {
    // Instead of getting a raw copy of a PgConnection, will get a pointer to one
    let db_pool = web::Data::new(db_pool);
    let server = HttpServer::new(move || {
        App::new()
            // Middlewares are added using the `wrap` method on `App`
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            // Register the connection as part of the application state,
            // and get a pointer copy and attach it to the application state
            .app_data(db_pool.clone())
    })
    .listen(listener)?
    .run();
    //no more .await
    Ok(server)
}
