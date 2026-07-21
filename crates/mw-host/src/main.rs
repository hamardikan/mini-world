use std::error::Error;
use std::net::SocketAddr;

use mw_host::http::router;
use mw_host::{bind_loopback, Host};

fn bind_addr() -> Result<SocketAddr, Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let mut value = None;
    while let Some(arg) = args.next() {
        if let Some(port) = arg.strip_prefix("--port=") {
            value = Some(port.to_string());
        } else if arg == "--port" {
            value = args.next();
        } else if !arg.starts_with('-') {
            value = Some(arg);
        } else {
            return Err(format!("unknown argument: {arg}").into());
        }
    }
    let port = value.as_deref().unwrap_or("7878");
    Ok(SocketAddr::from(([127, 0, 0, 1], port.parse()?)))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let addr = bind_addr()?;
    let std_listener = bind_loopback(addr)?;
    std_listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(std_listener)?;
    let bound_addr = listener.local_addr()?;

    let host = Host::with_default_config("local");
    // This round trip proves the actor has constructed its controller and is
    // ready before clients are told to connect.
    let _ = host.snapshot().await;
    println!("mw-host ready: http://{bound_addr}");

    let app = router(host.clone());
    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    });
    let result = server.await;
    host.shutdown();
    result?;
    Ok(())
}
