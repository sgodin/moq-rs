use std::collections::VecDeque;

use super::State;

pub struct Queue<T> {
    state: State<VecDeque<T>>,
}

impl<T> Queue<T> {
    /// Push an item onto the queue. Returns Err(item) if the queue has been closed.
    pub fn push(&mut self, item: T) -> Result<(), T> {
        match self.state.lock_mut() {
            Some(mut state) => state.push_back(item),
            None => return Err(item),
        };

        Ok(())
    }

    /// Pop an item from the queue, waiting if necessary.
    pub async fn pop(&mut self) -> Option<T> {
        loop {
            {
                let queue = self.state.lock();
                if !queue.is_empty() {
                    return queue.into_mut()?.pop_front();
                }
                queue.modified()?
            }
            .await;
        }
    }

    // Drop the state
    pub fn close(self) -> Vec<T> {
        // Drain the queue of any remaining entries
        let res = match self.state.lock_mut() {
            Some(mut queue) => queue.drain(..).collect(),
            _ => Vec::new(),
        };

        // Prevent any new entries from being added
        drop(self.state);

        res
    }

    /// Split the queue into two handles that share the same underlying state.
    pub fn split(self) -> (Self, Self) {
        let state = self.state.split();
        (Self { state: state.0 }, Self { state: state.1 })
    }
}

impl<T> Clone for Queue<T> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self {
            state: State::new(Default::default()),
        }
    }
}
