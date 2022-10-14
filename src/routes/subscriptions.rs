use actix_web::{web, HttpResponse};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use tracing::Instrument;

// We have to use the Deserialize macro from serde in order to extract FormData this way
// Form::from_request tries to deserialise the body into FormData according to the rules of URL-encoding
// leveraging serde_urlencoded and the Deserialize implementation of FormData, automatically generated for us by #[derive(serde::Deserialize)];
#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}
/* Test
curl -i -X POST -d 'email=thomas_mann@hotmail.com&name=Tom' \
    http://127.0.0.1:8000/subscriptions
*/
// All arguments in the signature of a route handler must implement the FromRequest trait: actix-web will invoke from_request for each argument.
/// Extract FormData using serde (via extractor using from_request trait method)
/// this handler gets called only if the content type is *x-www-form-urlencoded*
/// All arguments in the signature of a route handler must implement the FromRequest trait: actix-web will invoke from_request
/// for each argument and, if the extraction succeeds for all of
/// them, it will then run the actual handler function.
pub async fn subscribe(form: web::Form<FormData>, pg_pool: web::Data<PgPool>) -> HttpResponse {
    let request_id = Uuid::new_v4();
    let request_span = tracing::info_span!(
        "Adding a new subscriber.",
        %request_id,
        subscriber_email = %form.email,
        subscriber_name = %form.name
    );
    let _request_span_guard = request_span.enter();
    // We do not call `.enter` on query_span!
    // `.instrument` takes care of it at the right moments // in the query future lifetime
    let query_span = tracing::info_span!("Saving new subscriber details in the database");

    match sqlx::query!(
        r#"
    INSERT INTO subscriptions (id, email, name, subscribed_at) VALUES ($1, $2, $3, $4)
    "#,
        Uuid::new_v4(),
        form.email,
        form.name,
        Utc::now()
    )
    // We use `get_ref` to get an immutable reference to the `pg_pool`
    // wrapped by `web::Data`.
    // sqlx has an asynchronous interface, but it does not allow you to run multiple queries concurrently over the same database connection.
    // Requiring a mutable reference allows them to enforce this guarantee in their API.
    .execute(pg_pool.get_ref())
    // First we attach the instrumentation, then we `.await` it
    .instrument(query_span)
    .await
    {
        Ok(_) => {
            tracing::info!(
                "Correlation Id: {} > New subscriber details have been saved",
                request_id
            );
            HttpResponse::Ok().finish()
        }
        Err(e) => {
            tracing::error!(
                "Correlation Id: {} > Failed to execute query: {:?}",
                request_id,
                e
            );
            HttpResponse::InternalServerError().finish()
        }
    }
    /*
    The interesting thing about our PgConnection extractor, or extractors in general,
    is actix-web uses a type-map to represent its application state: a HashMap that stores
     arbitrary data (using the Any type) against their unique type identifier (obtained via TypeId::of).
    (Think of dependency injection technique from other languages)
    */
}
