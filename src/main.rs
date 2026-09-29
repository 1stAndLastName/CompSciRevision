use actix_web::{App, HttpServer, middleware, web};
use revision_site::{configure, load_site};
use std::net::{IpAddr, UdpSocket};

const PORT: u16 = 8080;

/// Which network address the server listens on.
///
/// `cargo run` makes a debug build, which listens on 0.0.0.0: every network
/// the computer is on, so a phone on the same Wi-Fi can open the site while
/// you test. A release build (`cargo build --release`, for hosting) listens
/// only on 127.0.0.1, this computer, and expects a web server such as nginx
/// in front of it. `cfg!(debug_assertions)` is true only in debug builds.
fn listen_address() -> &'static str {
    if cfg!(debug_assertions) {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    }
}

/// This computer's address on the local network (e.g. 192.168.0.34), if any.
///
/// "Connecting" a UDP socket sends nothing; it just asks the operating system
/// which of the computer's addresses it would use to reach the internet.
fn local_network_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:80").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load all content before starting. A broken file stops the server here
    // with a message naming the file, rather than breaking a page later.
    let library = match load_site() {
        Ok(library) => library,
        Err(error) => {
            eprintln!("Could not load content.\n  {error}");
            std::process::exit(1);
        }
    };

    let address = listen_address();
    println!(
        "Loaded {} topics and exam questions for {} spec points.",
        library.topics.len(),
        library.exam.len()
    );
    println!("  On this computer: http://localhost:{PORT}");
    // `&& let Some(ip) = ...` only runs the block if the address was found.
    if address == "0.0.0.0"
        && let Some(ip) = local_network_ip()
    {
        println!("  On a phone on the same Wi-Fi: http://{ip}:{PORT}");
    }

    // `web::Data` shares one read-only copy of the content between all the
    // worker threads (it is an `Arc` inside, a thread-safe shared pointer).
    let library = web::Data::new(library);

    HttpServer::new(move || {
        App::new()
            .app_data(library.clone())
            .wrap(middleware::DefaultHeaders::new().add(("X-Content-Type-Options", "nosniff")))
            .configure(configure)
    })
    .bind((address, PORT))?
    .run()
    .await
}
