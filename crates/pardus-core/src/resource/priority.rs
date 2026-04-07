//! Priority queue for resource scheduling

use std::{cmp::Ordering, collections::BinaryHeap};

/// Task with priority for the queue
#[derive(Debug, Clone)]
pub struct PriorityTask<T> {
    priority: u8,  // Lower = higher priority
    sequence: u64, // FIFO for equal priorities
    task: T,
}

impl<T> PriorityTask<T> {
    pub fn new(priority: u8, sequence: u64, task: T) -> Self {
        // Invert priority so BinaryHeap (max-heap) works as min-priority
        Self {
            priority,
            sequence,
            task,
        }
    }
}

impl<T> PartialEq for PriorityTask<T> {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.sequence == other.sequence
    }
}

impl<T> Eq for PriorityTask<T> {}

impl<T> PartialOrd for PriorityTask<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}

impl<T> Ord for PriorityTask<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering: lower priority value = higher actual priority
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| other.sequence.cmp(&self.sequence))
    }
}

/// Priority queue for resource tasks
#[derive(Debug)]
pub struct PriorityQueue<T> {
    heap: BinaryHeap<PriorityTask<T>>,
    sequence: u64,
}

impl<T> PriorityQueue<T> {
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
            sequence: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(capacity),
            sequence: 0,
        }
    }

    /// Push task with priority (0 = highest priority)
    pub fn push(&mut self, priority: u8, task: T) {
        self.sequence += 1;
        let pt = PriorityTask::new(priority, self.sequence, task);
        self.heap.push(pt);
    }

    /// Pop highest priority task
    pub fn pop(&mut self) -> Option<(u8, T)> { self.heap.pop().map(|pt| (pt.priority, pt.task)) }

    /// Peek at highest priority without removing
    pub fn peek(&self) -> Option<(&u8, &T)> { self.heap.peek().map(|pt| (&pt.priority, &pt.task)) }

    /// Number of tasks
    pub fn len(&self) -> usize { self.heap.len() }

    /// Is empty
    pub fn is_empty(&self) -> bool { self.heap.is_empty() }

    /// Drain all tasks
    pub fn drain(self) -> impl Iterator<Item = (u8, T)> {
        self.heap.into_iter().map(|pt| (pt.priority, pt.task))
    }

    /// Convert to sorted vec (highest priority first)
    pub fn into_vec(self) -> Vec<T> {
        let mut tasks: Vec<_> = self.drain().collect();
        tasks.sort_by_key(|(p, _)| *p);
        tasks.into_iter().map(|(_, t)| t).collect()
    }

    /// Clear all tasks
    pub fn clear(&mut self) {
        self.heap.clear();
        self.sequence = 0;
    }
}

impl<T> Default for PriorityQueue<T> {
    fn default() -> Self { Self::new() }
}

/// Multi-level priority queue
/// Different queues for different priority levels
#[derive(Debug)]
pub struct MultiLevelQueue<T> {
    critical: Vec<T>,   // Priority 0-31
    high: Vec<T>,       // Priority 32-95
    normal: Vec<T>,     // Priority 96-159
    low: Vec<T>,        // Priority 160-223
    background: Vec<T>, // Priority 224-255
}

impl<T> MultiLevelQueue<T> {
    pub fn new() -> Self {
        Self {
            critical: Vec::new(),
            high: Vec::new(),
            normal: Vec::new(),
            low: Vec::new(),
            background: Vec::new(),
        }
    }

    pub fn push(&mut self, priority: u8, task: T) {
        match priority {
            0..=31 => self.critical.push(task),
            32..=95 => self.high.push(task),
            96..=159 => self.normal.push(task),
            160..=223 => self.low.push(task),
            _ => self.background.push(task),
        }
    }

    /// Get all tasks in priority order
    pub fn all_tasks(self) -> impl Iterator<Item = T> {
        self.critical
            .into_iter()
            .chain(self.high)
            .chain(self.normal)
            .chain(self.low)
            .chain(self.background)
    }
}

impl<T> Default for MultiLevelQueue<T> {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_queue_ordering() {
        let mut queue = PriorityQueue::new();

        queue.push(5, "low");
        queue.push(1, "high");
        queue.push(3, "medium");
        queue.push(1, "high2"); // Same priority, should be FIFO

        let results: Vec<_> = queue.drain().collect();
        assert_eq!(results[0].0, 1); // First high priority
        assert_eq!(results[1].0, 1); // Second high priority
        assert_eq!(results[2].0, 3); // Medium
        assert_eq!(results[3].0, 5); // Low
    }

    #[test]
    fn test_multi_level_queue() {
        let mut queue = MultiLevelQueue::new();

        queue.push(200, "low");
        queue.push(10, "critical");
        queue.push(50, "high");
        queue.push(100, "normal");

        let tasks: Vec<_> = queue.all_tasks().collect();
        assert_eq!(tasks, vec!["critical", "high", "normal", "low"]);
    }
}
