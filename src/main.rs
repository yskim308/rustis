use std::env;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task;

use bytes::{Bytes, BytesMut};
use rustis::handler::CommandHandler;
use rustis::kv::KvStore;
use rustis::parser::{parse, BufParseError, ResponseValue};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

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

async fn handle_connection(stream: TcpStream, kv: KvStore) -> tokio::io::Result<()> {
    stream.set_nodelay(true)?;

    let (read_half, write_half) = stream.into_split();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    tokio::task::spawn_local(async move { writer_task(write_half, rx).await });

    reader_task(read_half, kv, tx).await?;

    Ok(())
}

async fn writer_task(
    mut write_half: OwnedWriteHalf,
    mut rx: UnboundedReceiver<Bytes>,
) -> tokio::io::Result<()> {
    while let Some(data) = rx.recv().await {
        write_half.write_all(&data).await?;
    }
    Ok(())
}

async fn reader_task(
    mut read_half: OwnedReadHalf,
    kv: KvStore,
    tx: UnboundedSender<Bytes>,
) -> tokio::io::Result<()> {
    let mut read_buffer = BytesMut::with_capacity(64 * 1024);
    let mut write_buffer = BytesMut::with_capacity(64 * 1024);
    let handler = CommandHandler::new(kv);

    // repeat until nothing to read
    loop {
        read_buffer.reserve(1024);
        if read_half.read_buf(&mut read_buffer).await? == 0 {
            break; //
        }

        loop {
            match parse(&mut read_buffer) {
                Ok(value) => {
                    handler.process_command(value).serialize(&mut write_buffer);
                }
                Err(BufParseError::Incomplete) => {
                    break;
                }
                Err(BufParseError::InvalidFirstByte(b)) => {
                    match b {
                        Some(byte) => {
                            let s = format!("-ERR invalid first byte: {}\r\n", byte);
                            write_buffer.extend_from_slice(s.as_bytes());
                        }
                        None => write_buffer.extend_from_slice(b"-ERR first byte not found \r\n"),
                    };
                    let _ = tx.send(write_buffer.freeze());
                    return Ok(()); // Close connection on protocol error
                }
                _ => {
                    write_buffer.extend_from_slice(b"-ERR internal server error\r\n");
                    let _ = tx.send(write_buffer.freeze());
                    return Ok(()); // Close connection on error
                }
            }
        }

        // flush after all possible frames are handled
        if tx.send(write_buffer.split().freeze()).is_err() {
            eprint!("error in reader task send");
            return Ok(()); // kill task if error
        };
    }

    Ok(())
}

// async fn handle_connection(mut stream: TcpStream, kv: KvStore) -> tokio::io::Result<()> {
//     stream.set_nodelay(true)?;
//
//     // Start small, grow as needed
//     let mut read_buffer = BytesMut::with_capacity(16 * 1024);
//     let mut write_buffer = BytesMut::with_capacity(16 * 1024);
//     let handler = CommandHandler::new(kv.clone());
//
//     loop {
//         // loop 1: read as much as possible
//         let mut first_read = true;
//         loop {
//             if first_read {
//                 if stream.read_buf(&mut read_buffer).await? == 0 {
//                     return Ok(());
//                 }
//                 first_read = false;
//             }
//
//             // Try to read more without blocking
//             read_buffer.reserve(16 * 1024);
//             match stream.try_read_buf(&mut read_buffer) {
//                 Ok(0) => return Ok(()), // EOF
//                 Ok(_) => continue,      // Got more data, keep reading
//                 Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
//                 Err(e) => return Err(e),
//             }
//         }
//
//         // loop 2: parse as much as possible
//         loop {
//             match parse(&mut read_buffer) {
//                 Ok(value) => {
//                     let response = handler.process_command(value);
//                     write_buffer.reserve(1024);
//                     response.serialize(&mut write_buffer);
//                 }
//                 Err(BufParseError::Incomplete) => break,
//                 _ => {
//                     stream.write_all(b"-ERR\r\n").await?;
//                     return Ok(());
//                 }
//             }
//         }
//
//         // Single write for all responses
//         if !write_buffer.is_empty() {
//             stream.write_all(&write_buffer).await?;
//             write_buffer.clear();
//         }
//     }
// }
