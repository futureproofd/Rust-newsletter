use crate::helpers::assert_is_redirect_to;
use crate::helpers::spawn_app;

#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    let app = spawn_app().await;

    let login_body = serde_json::json!({
        "username":"random_username",
        "password":"random_password",
    });

    let response = app.post_login(&login_body).await;
    // Assert
    assert_eq!(response.status().as_u16(), 303);
    assert_is_redirect_to(&response, "/login");

    let flash_cookie = response.cookies().find(|c| c.name() == "_flash").unwrap();
    assert_eq!(flash_cookie.value(), "Authentication failed");

    // Act - Part 2
    let html_page = app.get_login_html().await;
    assert!(html_page.contains(r#"<p><i>Authentication failed</i></p>"#));

    // Act - reload page, reset cookie
    let html_page = app.get_login_html().await;
    assert!(!html_page.contains(r#"Authentication failed"#));
}
