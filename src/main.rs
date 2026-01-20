use std::env;
use std::sync::Arc;

use rustis::handler::CommandHandler;
use rustis::kv::KvStore;
use rustis::parser::{BufParseError, Parser};

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

    let kv = Arc::new(KvStore::new());
    loop {
        let (stream, _) = listener.accept().await?;

        let kv_clone = kv.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, kv_clone).await {
                eprintln!("Error handling connection: {:?}", e);
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream, kv: Arc<KvStore>) -> tokio::io::Result<()> {
    let mut parser = Parser::new(4096);
    let handler = CommandHandler::new(kv.clone());
    loop {
        let bytes_read = stream.read_buf(&mut parser.buffer).await?;

        if bytes_read == 0 {
            return Ok(()); // Connection closed
        }

        loop {
            match parser.parse() {
                Ok(value) => {
                    println!("Parsed: {:?}", value);
                    let response = handler.process_command(value);
                    stream.write_all(&response.serialize()).await?;
                }
                Err(BufParseError::Incomplete) => {
                    // Not enough data yet, continue reading from the socket
                    break;
                }
                Err(e) => {
                    eprintln!("Parse error: {:?}", e);
                    return Ok(());
                }
            }
        }

        parser.compact();
    }
}
