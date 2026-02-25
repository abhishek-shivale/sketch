use axum::Router;
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
    let app = Router::new().fallback_service(
        ServeDir::new("public")
            .append_index_html_on_directories(true),
    );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();

    println!("Running on http://localhost:3000");

    axum::serve(listener, app).await.unwrap();
}