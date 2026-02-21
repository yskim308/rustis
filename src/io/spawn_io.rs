use std::{
    cell::RefCell,
    net,
    rc::Rc,
    sync::Arc,
};

use rtrb::Consumer;
use slab::Slab;
use tokio::sync::mpsc::UnboundedReceiver;
use crate::{
    core::{
        reply_dispatcher::ReplyDispatcher,
        shard_executor::ShardExecutor,
    },
    io::{connection_state::ConnectionState, io_poller::IOInboxPoller, reader_task::ReaderTask},
    message::{ResponseMessage, WorkerMessage},
    polling::{notified_ring_buffer::NotifiedProducer, task_notifier::TaskNotifier},
};

pub type WorkerQueues = Rc<RefCell<Vec<NotifiedProducer<WorkerMessage>>>>;

pub async fn spawn_io(
    core_id: usize,
    worker_queues: WorkerQueues,
    io_queues: Vec<Consumer<ResponseMessage>>,
    io_doorbell: Arc<TaskNotifier>,
    shard_executor: Rc<RefCell<ShardExecutor>>,
    reply_dispatcher: Rc<RefCell<ReplyDispatcher>>,
    mut conn_rx: UnboundedReceiver<net::TcpStream>,
) -> tokio::io::Result<()> {
    // create the registry of connections for this thread
    let connections = Rc::new(RefCell::new(Slab::<ConnectionState>::with_capacity(8192)));

    // spawn the inbox polling task
    let poller_connections = connections.clone();
    tokio::task::spawn_local(IOInboxPoller::new(
        poller_connections,
        io_queues,
        io_doorbell,
    ));

    // connection receive loop (accept happens in dedicated acceptor thread)
    loop {
        let std_stream = match conn_rx.recv().await {
            Some(stream) => stream,
            None => return Ok(()),
        };

        if let Err(e) = std_stream.set_nonblocking(true) {
            eprint!("error on core {} (set_nonblocking): {:?}", core_id, e);
            continue;
        }

        let stream = match tokio::net::TcpStream::from_std(std_stream) {
            Ok(stream) => stream,
            Err(e) => {
                eprint!("error on core {} (from_std): {:?}", core_id, e);
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
