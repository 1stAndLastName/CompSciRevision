use actix_web::{App, HttpServer, middleware, web};
use revision_site::{CONTENT_DIR, configure, content};
use std::path::Path;

const ADDRESS: (&str, u16) = ("127.0.0.1", 8080);

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load all content before starting. A broken file stops the server here
    // with a message naming the file, rather than breaking a page later.
    let library = match content::load(Path::new(CONTENT_DIR)) {
        Ok(library) => library,
        Err(error) => {
            eprintln!("Could not load content.\n  {error}");
            std::process::exit(1);
        }
    };
    println!(
        "Loaded {} topics. Open http://{}:{}",
        library.topics.len(),
        ADDRESS.0,
        ADDRESS.1
    );

    // `web::Data` shares one read-only copy of the content between all the
    // worker threads (it is an `Arc` inside, a thread-safe shared pointer).
    let library = web::Data::new(library);

    HttpServer::new(move || {
        App::new()
            .app_data(library.clone())
            .wrap(middleware::DefaultHeaders::new().add(("X-Content-Type-Options", "nosniff")))
            .configure(configure)
    })
    .bind(ADDRESS)?
    .run()
    .await
}
