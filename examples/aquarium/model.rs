use std::{collections::VecDeque, time::Instant};

use theoria::windows::{Win32QueuedMessage, Win32ThreadMessage, Win32WindowMessage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageStage {
    Queued,
    Thread,
    Window,
}

impl MessageStage {
    pub const fn index(self) -> usize {
        match self {
            Self::Queued => 0,
            Self::Thread => 1,
            Self::Window => 2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MessageObservation {
    pub stage: MessageStage,
    pub id: u32,
    pub wparam: usize,
    pub lparam: isize,
    pub destination: Option<usize>,
    pub posted_at: Option<u32>,
    pub cursor: Option<(i32, i32)>,
    pub observed_at: Instant,
}

impl MessageObservation {
    pub fn queued(message: &Win32QueuedMessage<'_>, observed_at: Instant) -> Self {
        let cursor = message.cursor_position();
        Self {
            stage: MessageStage::Queued,
            id: message.message().id(),
            wparam: message.message().wparam().0,
            lparam: message.message().lparam().0,
            destination: message.destination().map(|window| window.0 as usize),
            posted_at: Some(message.posted_at()),
            cursor: Some((cursor.x, cursor.y)),
            observed_at,
        }
    }

    pub fn thread(message: &Win32ThreadMessage<'_>, observed_at: Instant) -> Self {
        let cursor = message.cursor_position();
        Self {
            stage: MessageStage::Thread,
            id: message.message().id(),
            wparam: message.message().wparam().0,
            lparam: message.message().lparam().0,
            destination: None,
            posted_at: Some(message.posted_at()),
            cursor: Some((cursor.x, cursor.y)),
            observed_at,
        }
    }

    pub fn window(message: &Win32WindowMessage<'_>, observed_at: Instant) -> Self {
        Self {
            stage: MessageStage::Window,
            id: message.message().id(),
            wparam: message.message().wparam().0,
            lparam: message.message().lparam().0,
            destination: Some(message.window().0 as usize),
            posted_at: None,
            cursor: None,
            observed_at,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MessageEntry {
    pub sequence: u64,
    pub observation: MessageObservation,
}

pub struct MessageHistory {
    capacity: usize,
    next_sequence: u64,
    counts: [u64; 3],
    entries: VecDeque<MessageEntry>,
}

impl MessageHistory {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity != 0, "message history capacity must be positive");
        Self {
            capacity,
            next_sequence: 0,
            counts: [0; 3],
            entries: VecDeque::with_capacity(capacity),
        }
    }

    pub fn record(&mut self, observation: MessageObservation) {
        self.counts[observation.stage.index()] += 1;
        let entry = MessageEntry {
            sequence: self.next_sequence,
            observation,
        };
        self.next_sequence = self.next_sequence.wrapping_add(1);
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn count(&self, stage: MessageStage) -> u64 {
        self.counts[stage.index()]
    }

    pub fn entries(&self) -> impl DoubleEndedIterator<Item = &MessageEntry> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{MessageHistory, MessageObservation, MessageStage};

    fn observation(stage: MessageStage, id: u32) -> MessageObservation {
        MessageObservation {
            stage,
            id,
            wparam: 0,
            lparam: 0,
            destination: None,
            posted_at: None,
            cursor: None,
            observed_at: Instant::now(),
        }
    }

    #[test]
    fn bounded_history_keeps_the_latest_messages_without_losing_totals() {
        let mut history = MessageHistory::new(2);
        history.record(observation(MessageStage::Queued, 1));
        history.record(observation(MessageStage::Window, 2));
        history.record(observation(MessageStage::Queued, 3));

        assert_eq!(
            history
                .entries()
                .map(|entry| entry.observation.id)
                .collect::<Vec<_>>(),
            [2, 3]
        );
        assert_eq!(history.count(MessageStage::Queued), 2);
        assert_eq!(history.count(MessageStage::Thread), 0);
        assert_eq!(history.count(MessageStage::Window), 1);
    }
}
