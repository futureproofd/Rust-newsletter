use actix_web::{web, HttpResponse, ResponseError};
use actix_web::http::StatusCode;

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use chrono::Utc;

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use crate::{domain::{NewSubscriber, SubscriberName, SubscriberEmail}, email_client::EmailClient, startup::ApplicationBaseUrl};

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?; 
        let email = SubscriberEmail::parse(value.email)?; 
        Ok(Self { email, name })
    }
}


// #[from] automatically derives an implementation of From for the type it has been applied to into 
// the top-level error type (e.g. impl From<StoreTokenError> for SubscribeError {/* */}).
#[derive(thiserror::Error)] 
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error("{1}")]
    UnexpectedError(#[source] Box<dyn std::error::Error>, String)
}

// We are still using a bespoke implementation of `Debug` // to get a nice report using the error source chain
impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { 
        error_chain_fmt(self, f)
    } 
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST, 
            SubscribeError::UnexpectedError(_, _) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

pub struct StoreTokenError(sqlx::Error);

impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database failure was encountered while trying to store a subscription token."
        )
    }
}

// chain errors (currently not part of the std lib)
fn error_chain_fmt(e: &impl std::error::Error, f: &mut std::fmt::Formatter<'_>,) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}

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
    skip(form, pg_pool, email_client, base_url),
    fields(
        subscriber_email = %form.email, 
        subscriber_name= %form.name
    )
)]
// Get the email client from the app context
pub async fn subscribe(
    form: web::Form<FormData>,
    pg_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, SubscribeError> {
    // `web::Form` is a wrapper around `FormData`
    // `form.0` gives us access to the underlying `FormData`

    // We no longer have `#[from]` for `ValidationError`, (see thiserror macro) so we need to
    // map the error explicitly
    // That is because String does not implement the Error trait, therefore it can- not be returned in Error::source
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;

    // A mutable reference to a Transaction implements sqlx’s Executor trait therefore it can be used to run queries
    let mut transaction = pg_pool.begin()
        .await
        .map_err(|e| SubscribeError::UnexpectedError(Box::new(e), "Failed to acquire a Postgres connection from the pool".into(),))?;

    let subscriber_id =insert_subscriber(&mut transaction, &new_subscriber)
        .await
        .map_err(|e| SubscribeError::UnexpectedError(Box::new(e), "Failed to insert new subscriber in the database.".into()))?;

    let subscription_token = generate_subscription_token();
    
    // exit early if anything fails
    store_token(&mut transaction, subscriber_id, &subscription_token)
        .await
        .map_err(|e| SubscribeError::UnexpectedError(Box::new(e), 
            "Failed to store the confirmation token for a new subscriber.".into()
        ))?;

    // we need to manually commit the transaction to close the connection and stop rollbacks
    transaction.commit()
        .await
        .map_err(|e| SubscribeError::UnexpectedError(Box::new(e), 
            "Failed to commit SQL transaction to store a new subscriber.".into(),
        ))?;

    send_confirmation_email(&email_client, new_subscriber, &base_url.0, &subscription_token)
        .await
        .map_err(|e| SubscribeError::UnexpectedError(Box::new(e), "Failed to send a confirmation email.".into()))?;
    
    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber", skip(email_client, new_subscriber)
)]
pub async fn send_confirmation_email(email_client: &EmailClient, 
    new_subscriber: NewSubscriber, 
    base_url: &str,
    subscption_token: &str,
) -> Result<(), reqwest::Error>{
    let confirmation_link =
        format!("{}/subscriptions/confirm?subscription_token={}", base_url, subscption_token);

        let plain_body = format!(
            "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
            confirmation_link
        );
        let html_body = format!(
            "Welcome to our newsletter!<br />\
            Click <a href=\"{}\">here</a> to confirm your subscription.",
                  confirmation_link
        );
    // send an email to subscriber
     email_client.send_email(
        new_subscriber.email, "welcome title",
        &html_body,
        &plain_body,
    ).await
}

pub fn parse_subscriber(form: FormData) -> Result<NewSubscriber, String> {
    let name = SubscriberName::parse(form.name)?;
    let email = SubscriberEmail::parse(form.email)?;

    Ok(NewSubscriber {email, name})
}

#[tracing::instrument(
    name = "Saving new subscriber details to the database",
    skip(new_subscriber, transaction)
)]
pub async fn insert_subscriber(transaction: &mut Transaction<'_, Postgres>, new_subscriber: &NewSubscriber) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    sqlx::query!(
        r#"
    INSERT INTO subscriptions (id, email, name, subscribed_at, status) VALUES ($1, $2, $3, $4, 'pending_confirmation')
    "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now(),
    )
    // Before refactoring this, we used `get_ref` to get an immutable reference to the `pool` var
    // wrapped by `web::Data`.
    // sqlx has an asynchronous interface, but it does not allow you to run multiple queries concurrently over the same database connection.
    // Requiring a mutable reference allows them to enforce this guarantee in their API.
    .execute(transaction)
    .await
    .map_err(|err| {
        tracing::error!("Failed to execute query: {:?}", err);
        err

    // Using the `?` operator to return early
    // if the function failed, returning a sqlx::Error // We will talk about error handling in depth later!
    })?;
    Ok(subscriber_id)
}


/// Generate a random 25-characters-long case-sensitive subscription token.
fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
    .map(char::from)
    .take(25)
    .collect()
}

#[tracing::instrument(
name = "Store subscription token in the database", skip(subscription_token, transaction))]
pub async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    )
    .execute(transaction)
    .await
    .map_err(StoreTokenError)?; // (?) propagate upstream
    Ok(())
}

    /*
    The interesting thing about our PgConnection extractor, or extractors in general,
    is actix-web uses a type-map to represent its application state: a HashMap that stores
     arbitrary data (using the Any type) against their unique type identifier (obtained via TypeId::of).
    (Think of dependency injection technique from other languages)
    */