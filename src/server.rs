use std::path::Path;

use axum::Router;
use color_eyre::Result;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tower_livereload::LiveReloadLayer;

pub async fn run_server<P: AsRef<Path>>(output_dir: P, livereload: LiveReloadLayer) -> Result<()> {
    let static_files = ServeDir::new(output_dir);
    let router = Router::new()
        .fallback_service(static_files)
        .layer(livereload)
        .layer(TraceLayer::new_for_http());
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000").await?;
    println!("Listening on http://127.0.0.1:8000/");
    axum::serve(listener, router).await?;

    Ok(())
}
