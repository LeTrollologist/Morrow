use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:18081").await.unwrap();
        println!("=== Rust Tokio HTTP/1.1 Server ===");
        println!("Listening oo: http://127.0.0.1:18081");

        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                if let Ok(n) = socket.read(&mut buf).await {
                    if n > 0 {
                        let resp = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 31\r\nConnection: close\r\n\r\nHello from Rust Tokio Server!\n";
                        let _ = socket.write_all(resp).await;
                    }
                }
            });
        }
    });
    Ok(())
}
