use std::{cell::RefCell, rc::Rc, sync::Arc};

use bytes::Bytes;
use rtrb::{Consumer, RingBuffer};
use rustis::{
    connection::WorkerQueues,
    message::{RespFrame, WorkerMessage},
    polling::{notified_ring_buffer::NotifiedProducer, task_notifier::TaskNotifier},
    router::MessageRouter,
};

fn make_router(
    worker_count: usize,
    src_core: usize,
) -> (MessageRouter, Vec<Consumer<WorkerMessage>>) {
    let mut producers = Vec::with_capacity(worker_count);
    let mut consumers = Vec::with_capacity(worker_count);

    for _ in 0..worker_count {
        let (tx, rx) = RingBuffer::<WorkerMessage>::new(64);
        let notifier = Arc::new(TaskNotifier::new());
        producers.push(NotifiedProducer::new(tx, notifier));
        consumers.push(rx);
    }

    let queues: WorkerQueues = Rc::new(RefCell::new(producers));
    (MessageRouter::new(queues, 42, src_core), consumers)
}

fn pop_all(consumers: &mut [Consumer<WorkerMessage>]) -> Vec<WorkerMessage> {
    let mut out = Vec::new();
    for consumer in consumers {
        while let Ok(msg) = consumer.pop() {
            out.push(msg);
        }
    }
    out
}

fn bulk(s: &str) -> RespFrame {
    RespFrame::BulkString(Some(Bytes::copy_from_slice(s.as_bytes())))
}

#[test]
fn routes_keyed_command_to_exactly_one_worker() {
    let (router, mut consumers) = make_router(4, 0);
    let frame = RespFrame::Array(Some(vec![bulk("GET"), bulk("user:1")]));

    router.route_message(frame.clone(), 7);

    let messages = pop_all(&mut consumers);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.seq, 7);
    assert_eq!(msg.conn_token, 42);
    assert_eq!(msg.response_value, frame);
}

#[test]
fn rejects_empty_request_array() {
    let (router, mut consumers) = make_router(2, 1);

    router.route_message(RespFrame::Array(Some(vec![])), 5);

    let messages = pop_all(&mut consumers);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.src_core, 1);
    assert_eq!(msg.seq, 5);
    assert!(matches!(msg.response_value, RespFrame::Error(_)));
}

#[test]
fn ping_is_handled_as_direct_pong() {
    let (router, mut consumers) = make_router(3, 2);
    let frame = RespFrame::Array(Some(vec![bulk("PING")]));

    router.route_message(frame, 11);

    let messages = pop_all(&mut consumers);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.src_core, 2);
    assert_eq!(msg.seq, 11);
    assert_eq!(msg.response_value, RespFrame::SimpleString("PONG".into()));
}

#[test]
fn missing_key_for_keyed_command_returns_error() {
    let (router, mut consumers) = make_router(3, 0);
    let frame = RespFrame::Array(Some(vec![bulk("GET")]));

    router.route_message(frame, 13);

    let messages = pop_all(&mut consumers);
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.src_core, 0);
    assert_eq!(msg.seq, 13);
    assert!(matches!(msg.response_value, RespFrame::Error(_)));
}
