use actix_web::{web, HttpResponse};
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

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

/*
All instrumentation concerns are visually separated by execution concerns:
The first are dealt with in a procedural macro that “decorates”
the function declaration, while the function body focuses on the actual business logic.
*/
#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pg_pool),
    fields(
        subscriber_email = %form.email, 
        subscriber_name= %form.name
    )
)]
pub async fn subscribe(form: web::Form<FormData>, pg_pool: web::Data<PgPool>) -> HttpResponse {
    match insert_subscriber(&pg_pool, &form).await
    {
        Ok(_) =>  HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish()
    }
}

#[tracing::instrument(
    name = "Saving new subscriber details to the database",
    skip(form, pool)
)]
pub async fn insert_subscriber(pool: &PgPool, form: &FormData) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
    INSERT INTO subscriptions (id, email, name, subscribed_at) VALUES ($1, $2, $3, $4)
    "#,
        Uuid::new_v4(),
        form.email,
        form.name,
        Utc::now()
    )
    // Before refactoring this, we used `get_ref` to get an immutable reference to the `pool` var
    // wrapped by `web::Data`.
    // sqlx has an asynchronous interface, but it does not allow you to run multiple queries concurrently over the same database connection.
    // Requiring a mutable reference allows them to enforce this guarantee in their API.
    .execute(pool)
    .await
    .map_err(|err| {
        tracing::error!("Failed to execute query: {:?}", err);
        err

    // Using the `?` operator to return early
    // if the function failed, returning a sqlx::Error // We will talk about error handling in depth later!
    })?;
    Ok(())
}

    /*
    The interesting thing about our PgConnection extractor, or extractors in general,
    is actix-web uses a type-map to represent its application state: a HashMap that stores
     arbitrary data (using the Any type) against their unique type identifier (obtained via TypeId::of).
    (Think of dependency injection technique from other languages)
    */