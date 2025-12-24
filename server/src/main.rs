use tokio::net::TcpListener;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    
    let listener = TcpListener::bind(&addr).await?;
    println!("╔══════════════════════════════════════════════╗");
    println!("║   🚀 Chat Server Running on Port {}        ║", port);
    println!("╚══════════════════════════════════════════════╝");

    loop {
        let (socket, _) = listener.accept().await?;
        // Your existing connection handling logic goes here...
        // (If you have specific message logic, make sure to keep it!)
    }
}
