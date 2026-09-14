mod semantics {
    use std::{
        cell::RefCell,
        rc::{Rc, Weak},
    };

    use theoria_event::{Dispatch, DispatchOutcome, DispatchQueue, EventSink};

    type TestReceiver<E> = RefCell<Box<dyn FnMut(E)>>;

    mod bridge;
    mod channel;
    mod dispatch;
    mod queue;
    mod sink;
    mod sink_adapter;
    mod source;

    fn drain(queue: &mut DispatchQueue) {
        while queue.run_next().is_some() {}
    }

    pub(super) struct TestSinkOwner<E> {
        receiver: Rc<TestReceiver<E>>,
    }

    impl<E> TestSinkOwner<E> {
        pub(super) fn new(handler: impl FnMut(E) + 'static) -> Self {
            Self {
                receiver: Rc::new(RefCell::new(Box::new(handler))),
            }
        }
    }

    pub(super) struct TestSink<E> {
        receiver: Weak<TestReceiver<E>>,
    }

    impl<E> Clone for TestSink<E> {
        fn clone(&self) -> Self {
            Self {
                receiver: self.receiver.clone(),
            }
        }
    }

    impl<E> From<&TestSinkOwner<E>> for TestSink<E> {
        fn from(handler: &TestSinkOwner<E>) -> Self {
            Self {
                receiver: Rc::downgrade(&handler.receiver),
            }
        }
    }

    impl<E> EventSink<E> for TestSink<E> {
        type Dispatch = TestSinkDispatch<E>;

        fn send(self, event: E) -> Self::Dispatch {
            TestSinkDispatch {
                receiver: self.receiver,
                event,
            }
        }
    }

    impl<E> EventSink<E> for &TestSink<E> {
        type Dispatch = TestSinkDispatch<E>;

        fn send(self, event: E) -> Self::Dispatch {
            self.clone().send(event)
        }
    }

    pub(super) struct TestSinkDispatch<E> {
        receiver: Weak<TestReceiver<E>>,
        event: E,
    }

    impl<E> std::fmt::Debug for TestSinkDispatch<E> {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter
                .debug_struct("TestSinkDispatch")
                .finish_non_exhaustive()
        }
    }

    impl<E> Dispatch for TestSinkDispatch<E> {
        fn run(self) -> DispatchOutcome {
            let Some(receiver) = self.receiver.upgrade() else {
                return DispatchOutcome::SinkClosed;
            };
            receiver
                .try_borrow_mut()
                .expect("a test sink cannot be reentered")(self.event);
            DispatchOutcome::Completed
        }
    }
}
