mod semantics {
    use theoria_event::DispatchQueue;

    mod bridge;
    mod channel;
    mod dispatch;
    mod handler;
    mod queue;
    mod sink;
    mod sink_adapter;
    mod source;

    fn drain(queue: &mut DispatchQueue) {
        while queue.run_next().is_some() {}
    }
}
