// SPDX-License-Identifier: MIT

//! Play queue: ordering, shuffle and repeat logic (UI independent).

use std::sync::Arc;

use rand::seq::SliceRandom;

use crate::library::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Queue {
    /// Tracks in play order.
    items: Vec<Arc<Track>>,
    /// Order before shuffling, restored when shuffle is turned off.
    original: Option<Vec<Arc<Track>>>,
    pos: Option<usize>,
    pub repeat: Repeat,
}

impl Queue {
    #[must_use]
    pub fn items(&self) -> &[Arc<Track>] {
        &self.items
    }

    #[must_use]
    pub fn position(&self) -> Option<usize> {
        self.pos
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn shuffled(&self) -> bool {
        self.original.is_some()
    }

    #[must_use]
    pub fn current(&self) -> Option<&Arc<Track>> {
        self.pos.and_then(|p| self.items.get(p))
    }

    /// Replaces the queue and starts at `start`.
    pub fn set(&mut self, tracks: Vec<Arc<Track>>, start: usize, shuffle: bool) {
        self.items = tracks;
        self.original = None;
        self.pos = (!self.items.is_empty()).then_some(start.min(self.items.len() - 1));
        if shuffle {
            self.set_shuffle(true);
        }
    }

    /// Turns shuffle on (current track first, the rest shuffled) or off (original order).
    pub fn set_shuffle(&mut self, on: bool) {
        if on == self.shuffled() {
            return;
        }
        let current = self.current().cloned();
        if on {
            self.original = Some(self.items.clone());
            let mut rest: Vec<Arc<Track>> = self.items.clone();
            if let Some(p) = self.pos {
                rest.remove(p);
            }
            rest.shuffle(&mut rand::rng());
            self.items = current.iter().cloned().chain(rest).collect();
            self.pos = current.map(|_| 0);
        } else if let Some(original) = self.original.take() {
            self.items = original;
            self.pos = current.and_then(|c| self.items.iter().position(|t| Arc::ptr_eq(t, &c)));
        }
    }

    /// Index that follows the current one. `user` = explicit "next" (ignores repeat-one).
    #[must_use]
    pub fn next_index(&self, user: bool) -> Option<usize> {
        let pos = self.pos?;
        if !user && self.repeat == Repeat::One {
            return Some(pos);
        }
        if pos + 1 < self.items.len() {
            Some(pos + 1)
        } else if self.repeat != Repeat::Off && !self.items.is_empty() {
            Some(0)
        } else {
            None
        }
    }

    #[must_use]
    pub fn peek_next(&self) -> Option<&Arc<Track>> {
        self.next_index(false).and_then(|i| self.items.get(i))
    }

    /// Moves to the next track and returns it.
    pub fn advance(&mut self, user: bool) -> Option<Arc<Track>> {
        let next = self.next_index(user)?;
        self.pos = Some(next);
        self.current().cloned()
    }

    /// Moves to the previous track (wraps around with repeat-all).
    pub fn back(&mut self) -> Option<Arc<Track>> {
        let pos = self.pos?;
        let prev = if pos > 0 {
            pos - 1
        } else if self.repeat == Repeat::All {
            self.items.len().checked_sub(1)?
        } else {
            0
        };
        self.pos = Some(prev);
        self.current().cloned()
    }

    pub fn jump(&mut self, index: usize) -> Option<Arc<Track>> {
        (index < self.items.len()).then(|| {
            self.pos = Some(index);
            self.items[index].clone()
        })
    }

    /// Adds tracks at the end.
    pub fn append(&mut self, tracks: &[Arc<Track>]) {
        self.items.extend_from_slice(tracks);
        if let Some(original) = &mut self.original {
            original.extend_from_slice(tracks);
        }
        if self.pos.is_none() && !self.items.is_empty() {
            self.pos = Some(self.items.len() - tracks.len());
        }
    }

    /// Inserts tracks right after the current one.
    pub fn play_next(&mut self, tracks: &[Arc<Track>]) {
        let at = self.pos.map_or(self.items.len(), |p| p + 1);
        self.items.splice(at..at, tracks.iter().cloned());
        if let Some(original) = &mut self.original {
            let at = self
                .pos
                .and_then(|p| self.items.get(p))
                .and_then(|c| original.iter().position(|t| Arc::ptr_eq(t, c)))
                .map_or(original.len(), |p| p + 1);
            original.splice(at..at, tracks.iter().cloned());
        }
        if self.pos.is_none() && !self.items.is_empty() {
            self.pos = Some(at);
        }
    }

    /// Removes the item at `index`. Returns true when the current track was removed.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.items.len() {
            return false;
        }
        let removed = self.items.remove(index);
        if let Some(original) = &mut self.original {
            original.retain(|t| !Arc::ptr_eq(t, &removed));
        }
        match self.pos {
            Some(p) if p == index => {
                if self.items.is_empty() {
                    self.pos = None;
                } else {
                    self.pos = Some(p.min(self.items.len() - 1));
                }
                true
            }
            Some(p) if p > index => {
                self.pos = Some(p - 1);
                false
            }
            _ => false,
        }
    }

    /// Moves an item (drag & drop, up / down buttons).
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.items.len() || to >= self.items.len() || from == to {
            return;
        }
        let item = self.items.remove(from);
        self.items.insert(to, item);
        if let Some(p) = self.pos {
            self.pos = Some(if p == from {
                to
            } else if from < p && to >= p {
                p - 1
            } else if from > p && to <= p {
                p + 1
            } else {
                p
            });
        }
    }

    /// Drops everything except the current track.
    pub fn clear_upcoming(&mut self) {
        if let Some(current) = self.current().cloned() {
            self.items = vec![current];
            self.original = None;
            self.pos = Some(0);
        } else {
            self.clear();
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.original = None;
        self.pos = None;
    }

    /// Total remaining time after the current track.
    #[must_use]
    pub fn remaining_ms(&self) -> u64 {
        let start = self.pos.map_or(0, |p| p + 1);
        self.items.iter().skip(start).map(|t| t.duration_ms).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracks(n: i64) -> Vec<Arc<Track>> {
        (1..=n).map(|id| Arc::new(Track { id, ..Track::default() })).collect()
    }

    fn ids(q: &Queue) -> Vec<i64> {
        q.items().iter().map(|t| t.id).collect()
    }

    #[test]
    fn advance_and_repeat() {
        let mut q = Queue::default();
        q.set(tracks(3), 1, false);
        assert_eq!(q.current().unwrap().id, 2);
        assert_eq!(q.advance(false).unwrap().id, 3);
        assert!(q.advance(false).is_none());
        q.repeat = Repeat::All;
        assert_eq!(q.advance(false).unwrap().id, 1);
        q.repeat = Repeat::One;
        assert_eq!(q.advance(false).unwrap().id, 1);
        assert_eq!(q.advance(true).unwrap().id, 2);
        assert_eq!(q.back().unwrap().id, 1);
    }

    #[test]
    fn shuffle_keeps_current_and_restores_order() {
        let mut q = Queue::default();
        q.set(tracks(20), 5, false);
        q.set_shuffle(true);
        assert_eq!(q.position(), Some(0));
        assert_eq!(q.current().unwrap().id, 6);
        assert_eq!(q.len(), 20);
        q.advance(true);
        let now = q.current().unwrap().id;
        q.set_shuffle(false);
        assert_eq!(ids(&q), (1..=20).collect::<Vec<_>>());
        assert_eq!(q.current().unwrap().id, now);
    }

    #[test]
    fn play_next_remove_and_move() {
        let mut q = Queue::default();
        q.set(tracks(3), 0, false);
        q.play_next(&[Arc::new(Track { id: 9, ..Track::default() })]);
        assert_eq!(ids(&q), vec![1, 9, 2, 3]);
        assert!(!q.remove(2));
        assert_eq!(ids(&q), vec![1, 9, 3]);
        q.move_item(2, 0);
        assert_eq!(ids(&q), vec![3, 1, 9]);
        assert_eq!(q.current().unwrap().id, 1);
        assert!(q.remove(1));
        assert_eq!(q.current().unwrap().id, 9);
        q.clear_upcoming();
        assert_eq!(ids(&q), vec![9]);
    }

    #[test]
    fn append_to_empty_queue_starts_it() {
        let mut q = Queue::default();
        q.append(&tracks(2));
        assert_eq!(q.position(), Some(0));
        assert_eq!(q.remaining_ms(), 0);
    }
}
