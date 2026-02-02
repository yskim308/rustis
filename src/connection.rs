use std::{env, sync::Arc};

use bytes::{Bytes, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task,
};

use crate::{
    message::{ResponseMessage, ResponseValue, WorkerMessage},
    parser::{parse, BufParseError},
    router::route_message,
};

pub async fn spawn_io(router: Arc<Vec<UnboundedSender<WorkerMessage>>>) -> tokio::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let port = args
        .get(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(6379);
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on port {port}");

    let local = task::LocalSet::new();

    local
        .run_until(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();

                let router_clone = router.clone();
                tokio::task::spawn_local(async move {
                    if let Err(e) = handle_connection(stream, &router_clone).await {
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

async fn handle_connection(
    stream: TcpStream,
    router: &[UnboundedSender<WorkerMessage>],
) -> tokio::io::Result<()> {
    stream.set_nodelay(true)?;

    let (read_half, write_half) = stream.into_split();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    tokio::task::spawn_local(async move { writer_task(write_half, rx).await });

    reader_task(read_half, tx, router).await?;

    Ok(())
}

async fn writer_task(
    mut write_half: OwnedWriteHalf,
    mut rx: UnboundedReceiver<ResponseMessage>,
) -> tokio::io::Result<()> {
    let mut last_seq: u64 = 0;
    let mut buffer = std::collections::BTreeMap::new();
    let mut write_buffer = BytesMut::with_capacity(64 * 1024);
    while let Some(first_message) = rx.recv().await {
        // collect message from recv
        buffer.insert(first_message.seq, first_message.response_value);

        // drain any message currently in channel
        while let Ok(msg) = rx.try_recv() {
            buffer.insert(msg.seq, msg.response_value);
        }

        // write to write buffer
        while let Some(response_value) = buffer.remove(&(last_seq + 1)) {
            response_value.serialize(&mut write_buffer);
            last_seq += 1;
        }

        if !write_buffer.is_empty() {
            write_half.write_all(&write_buffer).await?;
            write_buffer.clear();
        }
    }
    Ok(())
}

async fn reader_task(
    mut read_half: OwnedReadHalf,
    tx: UnboundedSender<ResponseMessage>,
    router: &[UnboundedSender<WorkerMessage>],
) -> tokio::io::Result<()> {
    let mut read_buffer = BytesMut::with_capacity(64 * 1024);

    let mut seq: u64 = 0;
    loop {
        read_buffer.reserve(1024);
        if read_half.read_buf(&mut read_buffer).await? == 0 {
            break; //
        }

        loop {
            match parse(&mut read_buffer) {
                Ok(value) => {
                    seq += 1;
                    let tx_clone = tx.clone();
                    route_message(router, value, seq, tx_clone);
                }
                Err(BufParseError::Incomplete) => {
                    break;
                }
                Err(BufParseError::InvalidFirstByte(b)) => {
                    match b {
                        Some(byte) => {
                            let s = format!("-ERR invalid first byte: {}", byte);
                            let _ = tx.send(ResponseMessage {
                                seq,
                                response_value: ResponseValue::Error(s.into()),
                            });
                        }
                        None => {
                            let _ = tx.send(ResponseMessage {
                                seq,
                                response_value: ResponseValue::Error(
                                    "ERR first byte not found".into(),
                                ),
                            });
                        }
                    };
                    return Ok(()); // Close connection on protocol error
                }
                _ => {
                    let _ = tx.send(ResponseMessage {
                        seq,
                        response_value: ResponseValue::Error("ERR internal server error".into()),
                    });
                    return Ok(()); // Close connection on error
                }
            }
        }
    }

    Ok(())
}
