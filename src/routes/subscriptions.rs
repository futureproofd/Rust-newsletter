use actix_web::{web, HttpResponse};
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
pub async fn subscribe(form: web::Form<FormData>, 
    pg_pool: web::Data<PgPool>, 
    email_client: web::Data<EmailClient>, 
    base_url: web::Data<ApplicationBaseUrl>
) -> HttpResponse {
     // `web::Form` is a wrapper around `FormData`
    // `form.0` gives us access to the underlying `FormData`
    let new_subscriber = match form.0.try_into() {
        Ok(subscriber) => subscriber,
        Err(_) => return HttpResponse::BadRequest().finish()
    };

    // A mutable reference to a Transaction implements sqlx’s Executor trait therefore it can be used to run queries
    let mut transaction = match pg_pool.begin().await {
        Ok(transaction) => transaction,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscriber_id = match insert_subscriber(&mut transaction, &new_subscriber).await {
        Ok(subscriber_id) => subscriber_id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    let subscription_token = generate_subscription_token();
    
    // exit early if anything fails
    if store_token(&mut transaction, subscriber_id, &subscription_token).await.is_err(){
        return HttpResponse::InternalServerError().finish();
    }

    // we need to manually commit the transaction to close the connection and stop rollbacks
    if transaction.commit().await.is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    if send_confirmation_email(&email_client, new_subscriber, &base_url.0, &subscription_token).await.is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().finish()
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

) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    )
    .execute(transaction)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e })?;
    Ok(())
}

    /*
    The interesting thing about our PgConnection extractor, or extractors in general,
    is actix-web uses a type-map to represent its application state: a HashMap that stores
     arbitrary data (using the Any type) against their unique type identifier (obtained via TypeId::of).
    (Think of dependency injection technique from other languages)
    */