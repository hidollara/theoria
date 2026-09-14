mod semantics {
    use theoria_event::FifoDispatcher;

    mod bridge;
    mod channel;
    mod dispatch;
    mod dispatcher;
    mod handler;
    mod sink;
    mod sink_adapter;
    mod source;

    fn drain(dispatcher: &mut FifoDispatcher) {
        while dispatcher.deliver_next() {}
    }
}
