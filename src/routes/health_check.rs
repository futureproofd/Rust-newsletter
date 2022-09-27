use actix_web::HttpResponse;

// curl -v http://127.0.0.1:8000/health_check
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().finish()
}
