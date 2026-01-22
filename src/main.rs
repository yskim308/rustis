use std::env;
use tokio::task;

use bytes::BytesMut;
use rustis::handler::CommandHandler;
use rustis::kv::KvStore;
use rustis::parser::{parse, BufParseError};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main(flavor = "current_thread")]
async fn main() -> tokio::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let port = args
        .get(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(6379);
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on port {port}");

    let kv = KvStore::new();

    let local = task::LocalSet::new();

    local
        .run_until(async move {
            loop {
                // todo: handle unwrap
                let (stream, _) = listener.accept().await.unwrap();

                let kv_clone = kv.clone();
                tokio::task::spawn_local(async move {
                    if let Err(e) = handle_connection(stream, kv_clone).await {
                        match e.kind() {
                            std::io::ErrorKind::ConnectionReset => {}
                            _ => eprintln!("Error handling connection: {:?}", e),
                        }
                    }
                });
            }
        })
        .await;
    Ok(())
}

async fn handle_connection(mut stream: TcpStream, kv: KvStore) -> tokio::io::Result<()> {
    let mut read_buffer = BytesMut::with_capacity(16 * 1024);
    let mut write_buffer = BytesMut::with_capacity(16 * 1024);
    let handler = CommandHandler::new(kv.clone());
    loop {
        let bytes_read = stream.read_buf(&mut read_buffer).await?;

        if bytes_read == 0 {
            return Ok(()); // Connection closed
        }

        loop {
            match parse(&mut read_buffer) {
                Ok(value) => {
                    let response = handler.process_command(value);
                    response.serialize(&mut write_buffer);
                }
                Err(BufParseError::Incomplete) => {
                    // Not enough data yet, continue reading from the socket
                    break;
                }
                Err(BufParseError::InvalidFirstByte(b)) => {
                    match b {
                        Some(byte) => {
                            stream
                                .write_all(format!("-invalid first byte: {}\r\n", byte).as_bytes())
                                .await?
                        }
                        None => stream.write_all(b"-first byte not found\r\n").await?,
                    };
                    return Ok(());
                }
                _ => {
                    stream.write_all(b"-internal server error\r\n").await?;
                    return Ok(());
                }
            }
        }

        if !write_buffer.is_empty() {
            stream.write_all(&write_buffer).await?;
            write_buffer.clear();
        }
    }
}
