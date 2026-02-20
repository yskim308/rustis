pub async fn reader_task(
    mut read_half: OwnedReadHalf,
    worker_queues: WorkerQueues,
    conn_token: usize,
    core_id: usize,
    shard_executor: Rc<RefCell<ShardExecutor>>,
    reply_dispatcher: Rc<RefCell<ReplyDispatcher>>,
) -> tokio::io::Result<()> {
    let mut read_buffer = BytesMut::with_capacity(64 * 1024);

    // each connection gets their own stateful router
    let router = MessageRouter::new(worker_queues, conn_token, core_id, shard_executor);

    let mut seq: u64 = 0;
    loop {
        read_buffer.reserve(1024);
        if read_half.read_buf(&mut read_buffer).await? == 0 {
            break; //
        }

        loop {
            match parse(&mut read_buffer) {
                Ok(value) => {
                    #[cfg(debug_assertions)]
                    println!("parsed: {:?}", value);

                    router.route_message(value, seq);
                    seq += 1;
                }
                Err(BufParseError::Incomplete) => {
                    break;
                }
                Err(BufParseError::InvalidFirstByte(b)) => {
                    match b {
                        Some(byte) => {
                            let s = format!("ERR invalid first byte: {}", byte);
                            router.route_message(RespFrame::Error(s.into()), seq);
                        }
                        None => router.route_message(
                            RespFrame::Error("ERR first byte not found".into()),
                            seq,
                        ),
                    };
                    return Ok(()); // Close connection on protocol error
                }
                _ => {
                    router.route_message(RespFrame::Error("ERR internal server error".into()), seq);
                    return Ok(()); // Close connection on error
                }
            }
        }
    }

    Ok(())
}
