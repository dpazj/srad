use std::collections::{btree_map::Entry, BTreeMap};

/// Result from `Resequencer::drain`
#[allow(dead_code)]
pub enum DrainResult<T> {
    /// The next in order message
    Message(T),
    /// No more messages
    Empty,
    /// The `Resequencer`` can not provide any more in order messages
    SequenceMissing,
}

/// Result from `Resequencer::process`
#[allow(dead_code)]
pub enum ProcessResult<T> {
    /// The message provided was the next expected message in the sequence
    MessageNextInSequence(T),
    /// The message provided was out of order
    OutOfSequenceMessageInserted,
    /// The resequencer already has a message with that sequence value
    DuplicateMessageSequence,
}

enum State {
    Good,
    ReSequencing(u8),
}

/// A buffer used to validate the order of and re-sequence Sparkplug messages
pub struct Resequencer<T> {
    buffer: BTreeMap<u8, T>,
    next_seq: u8,
    state: State,
}

impl<T> Resequencer<T> {
    /// Makes a new empty `Resequencer`
    pub fn new() -> Self {
        Self {
            buffer: BTreeMap::new(),
            next_seq: 0,
            state: State::Good,
        }
    }

    /// Set the next expected sequence value
    pub fn set_next_sequence(&mut self, next_seq: u8) {
        self.next_seq = next_seq
    }

    /// Get the next expected seq value
    pub fn next_sequence(&self) -> u8 {
        self.next_seq
    }

    fn increment_next_seq(&mut self) {
        self.next_seq = self.next_seq.wrapping_add(1);
    }

    /// Provide a new message to the `Resequencer` to be validated.
    ///
    /// If the message is the next expected message in the sequence then it will be returned as a `ProcessResult::MessageNextInSequence` value.
    /// If the message is not the next expected message and it's sequence value is valid `ProcessResult::OutOfSequenceMessageInserted` will be returned.
    /// If the message is a duplicate and cannot be re-sequenced, then `ProcessResult::DuplicateMessageSequence` will be returned.
    pub fn process(&mut self, seq: u8, message: T) -> ProcessResult<T> {
        if self.next_seq == seq {
            /* Prevent processing a state we already have in our buffer*/
            if let State::ReSequencing(offset) = self.state {
                if self.buffer.contains_key(&seq.wrapping_sub(offset)) {
                    return ProcessResult::DuplicateMessageSequence;
                }
            }

            self.increment_next_seq();
            return ProcessResult::MessageNextInSequence(message);
        }

        let offset = match self.state {
            State::ReSequencing(offset) => offset,
            State::Good => {
                self.state = State::ReSequencing(self.next_seq);
                self.next_seq
            }
        };

        match self.buffer.entry(seq.wrapping_sub(offset)) {
            Entry::Vacant(entry) => {
                entry.insert(message);
                ProcessResult::OutOfSequenceMessageInserted
            }
            Entry::Occupied(_) => ProcessResult::DuplicateMessageSequence,
        }
    }

    /// Try get the next in order message from the buffer.
    ///
    /// `DrainResult::MessageNextInSequence` will be returned if the `Resequencer` can provide the next sequenced message.
    /// Will return `DrainResult::Empty` when there are no more messages in the buffer.
    /// `DrainResult::SequenceMissing` will be returned when there are no more messages that can be provided in order.
    pub fn drain(&mut self) -> DrainResult<T> {
        let offset = match self.state {
            State::Good => {
                assert!(self.buffer.is_empty());
                return DrainResult::Empty;
            }
            State::ReSequencing(offset) => offset,
        };

        if let Some((key, _)) = self.buffer.first_key_value() {
            if key.wrapping_add(offset) != self.next_seq {
                return DrainResult::SequenceMissing;
            }
        } else {
            return DrainResult::Empty;
        }

        let (_, message) = self.buffer.pop_first().unwrap();
        self.increment_next_seq();

        if self.buffer.is_empty() {
            self.state = State::Good
        }

        DrainResult::Message(message)
    }

    /// Reset the state of the `Resequencer`. This will also clear the internal buffer.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.next_seq = 0;
        self.state = State::Good
    }
}

impl<T> Default for Resequencer<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::resequencer::{DrainResult, ProcessResult};

    use super::Resequencer;

    struct TmpMessage {}

    impl TmpMessage {
        pub fn new() -> Self {
            Self {}
        }
    }

    #[test]
    fn good_order() {
        let mut re = Resequencer::new();

        //Test wrapping
        for i in 0..=u8::MAX {
            assert_eq!(re.next_sequence(), i);
            assert!(matches!(
                re.process(i, TmpMessage::new()),
                ProcessResult::MessageNextInSequence(_)
            ));
            assert!(matches!(re.drain(), DrainResult::Empty));
        }
        assert_eq!(re.next_sequence(), 0);
        assert!(matches!(
            re.process(0, TmpMessage::new()),
            ProcessResult::MessageNextInSequence(_)
        ));
        assert!(matches!(re.drain(), DrainResult::Empty));
    }

    #[test]
    fn out_of_sequence() {
        let mut re = Resequencer::new();
        assert!(matches!(
            re.process(0, TmpMessage::new()),
            ProcessResult::MessageNextInSequence(_)
        ));
        assert!(matches!(re.drain(), DrainResult::Empty));

        assert_eq!(re.next_sequence(), 1);
        assert!(matches!(
            re.process(2, TmpMessage::new()),
            ProcessResult::OutOfSequenceMessageInserted
        ));
        assert!(matches!(re.drain(), DrainResult::SequenceMissing));

        assert_eq!(re.next_sequence(), 1);
        assert!(matches!(
            re.process(1, TmpMessage::new()),
            ProcessResult::MessageNextInSequence(_)
        ));
        assert_eq!(re.next_sequence(), 2);
        assert!(matches!(re.drain(), DrainResult::Message(_)));
        assert_eq!(re.next_sequence(), 3);

        assert!(matches!(
            re.process(5, TmpMessage::new()),
            ProcessResult::OutOfSequenceMessageInserted
        ));
        assert!(matches!(
            re.process(4, TmpMessage::new()),
            ProcessResult::OutOfSequenceMessageInserted
        ));
        assert!(matches!(
            re.process(7, TmpMessage::new()),
            ProcessResult::OutOfSequenceMessageInserted
        ));
        assert!(matches!(
            re.process(8, TmpMessage::new()),
            ProcessResult::OutOfSequenceMessageInserted
        ));

        assert!(matches!(
            re.process(3, TmpMessage::new()),
            ProcessResult::MessageNextInSequence(_)
        ));
        assert_eq!(re.next_sequence(), 4);
        assert!(matches!(re.drain(), DrainResult::Message(_)));
        assert!(matches!(re.drain(), DrainResult::Message(_)));
        assert!(matches!(re.drain(), DrainResult::SequenceMissing));
        assert_eq!(re.next_sequence(), 6);

        assert!(matches!(
            re.process(6, TmpMessage::new()),
            ProcessResult::MessageNextInSequence(_)
        ));
        assert!(matches!(re.drain(), DrainResult::Message(_)));
        assert!(matches!(re.drain(), DrainResult::Message(_)));
        assert!(matches!(re.drain(), DrainResult::Empty));

        assert!(matches!(
            re.process(9, TmpMessage::new()),
            ProcessResult::MessageNextInSequence(_)
        ));
        assert!(matches!(
            re.process(11, TmpMessage::new()),
            ProcessResult::OutOfSequenceMessageInserted
        ));
        assert!(matches!(
            re.process(10, TmpMessage::new()),
            ProcessResult::MessageNextInSequence(_)
        ));
        assert!(matches!(re.drain(), DrainResult::Message(_)));
        assert!(matches!(re.drain(), DrainResult::Empty));
    }

    #[test]
    fn out_of_sequence_duplicate() {
        let mut re = Resequencer::new();
        assert!(matches!(
            re.process(0, TmpMessage::new()),
            ProcessResult::MessageNextInSequence(_)
        ));
        assert_eq!(re.next_sequence(), 1);
        assert!(matches!(
            re.process(3, TmpMessage::new()),
            ProcessResult::OutOfSequenceMessageInserted
        ));
        assert!(matches!(
            re.process(2, TmpMessage::new()),
            ProcessResult::OutOfSequenceMessageInserted
        ));
        assert!(matches!(
            re.process(1, TmpMessage::new()),
            ProcessResult::MessageNextInSequence(_)
        ));
        assert!(matches!(
            re.process(2, TmpMessage::new()),
            ProcessResult::DuplicateMessageSequence
        ));
    }

    #[test]
    fn out_of_sequence_wrapping() {
        let mut re = Resequencer::new();

        //Test wrapping
        for i in 0..u8::MAX {
            assert_eq!(re.next_sequence(), i);
            assert!(matches!(
                re.process(i, TmpMessage::new()),
                ProcessResult::MessageNextInSequence(_)
            ));
            assert!(matches!(re.drain(), DrainResult::Empty));
        }

        assert_eq!(re.next_sequence(), 255);
        assert!(matches!(
            re.process(0, TmpMessage::new()),
            ProcessResult::OutOfSequenceMessageInserted
        ));

        assert_eq!(re.next_sequence(), 255);
        assert!(matches!(
            re.process(u8::MAX, TmpMessage::new()),
            ProcessResult::MessageNextInSequence(_)
        ));
        assert_eq!(re.next_sequence(), 0);
        assert!(matches!(re.drain(), DrainResult::Message(_)));
        assert_eq!(re.next_sequence(), 1);
        assert!(matches!(re.drain(), DrainResult::Empty));
    }
}
