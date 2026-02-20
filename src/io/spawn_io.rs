use std::{
    cell::RefCell,
    env,
    net::{self},
    rc::Rc,
    sync::{atomic::Ordering, Arc},
    task::Poll,
};

use bytes::{Buf, BytesMut};
use rtrb::Consumer;
use slab::Slab;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{
    io::AsyncReadExt,
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::{
    core::{
        reply_dispatcher::{self, ReplyDispatcher},
        shard_executor::ShardExecutor,
    },
    io::{connection_state::ConnectionState, io_poller::IOInboxPoller, reader_task::ReaderTask},
    message::{RespFrame, ResponseMessage, WorkerMessage},
    parser::{parse, BufParseError},
    polling::{notified_ring_buffer::NotifiedProducer, task_notifier::TaskNotifier},
    router::MessageRouter,
};

pub type WorkerQueues = Rc<RefCell<Vec<NotifiedProducer<WorkerMessage>>>>;

pub async fn spawn_io(
    core_id: usize,
    worker_queues: WorkerQueues,
    io_queues: Vec<Consumer<ResponseMessage>>,
    io_doorbell: Arc<TaskNotifier>,
    shard_executor: Rc<RefCell<ShardExecutor>>,
    reply_dispatcher: Rc<RefCell<ReplyDispatcher>>,
) -> tokio::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let port = args
        .get(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(6379);
    let addr = format!("127.0.0.1:{}", port);
    let std_addr: net::SocketAddr = addr
        .parse()
        .expect("failure while parsing address for socket");
    let socket2_addr: socket2::SockAddr = std_addr.into();

    // set up socket (note: reuse port only works on unix machines)
    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
        .expect("failed to create socket2 socket");
    socket
        .set_reuse_address(true)
        .expect("failed to set reuse socket2 address");
    socket
        .set_reuse_port(true)
        .expect("failed to set reuse port");
    socket
        .bind(&socket2_addr)
        .expect("failed to bind to socket2 address");
    socket
        .listen(1024)
        .expect("failed to listen and set 1024 backlog");

    let std_listener: net::TcpListener = socket.into();
    std_listener
        .set_nonblocking(true)
        .expect("failed while setting TCP litener to non blocking");
    let listener = tokio::net::TcpListener::from_std(std_listener)
        .expect("failed to create async listener from std listener");

    println!("Listening on port {port}");

    // create the registry of connections for this thread
    let connections = Rc::new(RefCell::new(Slab::<ConnectionState>::with_capacity(8192)));

    // spawn the inbox polling task
    let poller_connections = connections.clone();
    tokio::task::spawn_local(IOInboxPoller::new(
        poller_connections,
        io_queues,
        io_doorbell,
    ));

    // connection accepting loop
    loop {
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprint!("error on core {}: {:?}", core_id, e);
                continue;
            }
        };

        let (read_half, write_half) = stream.into_split();

        let token = {
            let mut reader_connections = connections.borrow_mut();
            reader_connections.insert(ConnectionState::new(write_half))
        };

        #[cfg(debug_assertions)]
        println!("connection accepted with token: {}", token);

        let cloned_worker_queues = worker_queues.clone();
        let cloned_connections = connections.clone();
        let cloned_shard_executor = shard_executor.clone();
        let cloend_reply_dispatcher = reply_dispatcher.clone();

        // pass in token to reader task
        tokio::task::spawn_local(async move {
            let mut reader_task = ReaderTask::new(
                read_half,
                cloned_worker_queues,
                token,
                core_id,
                cloned_shard_executor,
                cloend_reply_dispatcher,
            );

            if let Err(err) = reader_task.run().await {
                eprintln!("reader_task error (conn {token}): {err}");
            }
            cloned_connections.borrow_mut().remove(token);
        });
    }
}
