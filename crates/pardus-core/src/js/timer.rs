

#[derive(Debug, Clone)]
pub struct TimerEntry {
    pub id: u32,
    pub callback_str: Option<String>,
    pub delay_ms: u64,
    pub is_interval: bool,
    pub is_fired: bool,
}

#[derive(Debug)]
pub struct TimerQueue {
    timers: Vec<TimerEntry>,
    next_id: u32,
    max_ticks: u32,
    tick_count: u32,
}

impl TimerQueue {
    pub fn new() -> Self {
        Self {
            timers: Vec::new(),
            next_id: 1,
            max_ticks: 1000,
            tick_count: 0,
        }
    }

    pub fn set_timeout(&mut self, callback_str: Option<String>, delay_ms: u64) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.timers.push(TimerEntry {
            id,
            callback_str,
            delay_ms,
            is_interval: false,
            is_fired: false,
        });
        id
    }

    pub fn set_interval(&mut self, callback_str: Option<String>, delay_ms: u64) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.timers.push(TimerEntry {
            id,
            callback_str,
            delay_ms,
            is_interval: true,
            is_fired: false,
        });
        id
    }

    pub fn clear_timer(&mut self, id: u32) {
        if let Some(pos) = self.timers.iter().position(|t| t.id == id) {
            self.timers.remove(pos);
        }
    }

    pub fn tick_count(&self) -> u32 {
        self.tick_count
    }

    pub fn is_at_limit(&self) -> bool {
        self.tick_count >= self.max_ticks
    }

    pub fn get_expired_timer_callbacks_js(&self) -> String {
        let mut js = String::new();
        for timer in &self.timers {
            if timer.is_fired {
                continue;
            }
            if timer.delay_ms == 0 {
                if let Some(cb) = &timer.callback_str {
                    js.push_str(&format!(
                        "try {{ (function() {{ {} }})(); }} catch(e) {{ }}\n",
                        cb
                    ));
                }
            }
        }
        js
    }

    pub fn mark_delay_zero_fired(&mut self) {
        for timer in &mut self.timers {
            if timer.delay_ms == 0 && !timer.is_interval && !timer.is_fired {
                timer.is_fired = true;
                self.tick_count += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_timeout_returns_incrementing_ids() {
        let mut queue = TimerQueue::new();
        let id1 = queue.set_timeout(Some("a".into()), 0);
        let id2 = queue.set_timeout(Some("b".into()), 100);
        let id3 = queue.set_timeout(Some("c".into()), 0);
        assert!(id1 < id2);
        assert!(id2 < id3);
    }

    #[test]
    fn test_set_interval_returns_incrementing_ids() {
        let mut queue = TimerQueue::new();
        let id1 = queue.set_interval(Some("a".into()), 100);
        let id2 = queue.set_interval(Some("b".into()), 200);
        assert!(id1 < id2);
    }

    #[test]
    fn test_set_timeout_creates_non_interval() {
        let mut queue = TimerQueue::new();
        queue.set_timeout(Some("cb".into()), 0);
        assert_eq!(queue.timers.len(), 1);
        assert!(!queue.timers[0].is_interval);
        assert_eq!(queue.timers[0].delay_ms, 0);
    }

    #[test]
    fn test_set_interval_creates_interval() {
        let mut queue = TimerQueue::new();
        queue.set_interval(Some("cb".into()), 500);
        assert_eq!(queue.timers.len(), 1);
        assert!(queue.timers[0].is_interval);
        assert_eq!(queue.timers[0].delay_ms, 500);
    }

    #[test]
    fn test_clear_timer_removes_timer() {
        let mut queue = TimerQueue::new();
        let id = queue.set_timeout(Some("cb".into()), 0);
        assert_eq!(queue.timers.len(), 1);
        queue.clear_timer(id);
        assert!(queue.timers.is_empty());
    }

    #[test]
    fn test_clear_timer_nonexistent_is_noop() {
        let mut queue = TimerQueue::new();
        queue.set_timeout(Some("cb".into()), 0);
        queue.clear_timer(999);
        assert_eq!(queue.timers.len(), 1);
    }

    #[test]
    fn test_clear_timer_by_id_not_position() {
        let mut queue = TimerQueue::new();
        let id1 = queue.set_timeout(Some("a".into()), 0);
        let _id2 = queue.set_timeout(Some("b".into()), 0);
        let id3 = queue.set_timeout(Some("c".into()), 0);
        queue.clear_timer(id1);
        assert_eq!(queue.timers.len(), 2);
        assert_eq!(queue.timers[0].callback_str, Some("b".into()));
        assert_eq!(queue.timers[1].callback_str, Some("c".into()));
        // id3 still works
        queue.clear_timer(id3);
        assert_eq!(queue.timers.len(), 1);
    }

    #[test]
    fn test_get_expired_timer_callbacks_js_empty() {
        let queue = TimerQueue::new();
        assert_eq!(queue.get_expired_timer_callbacks_js(), "");
    }

    #[test]
    fn test_get_expired_timer_callbacks_js_delay_zero() {
        let mut queue = TimerQueue::new();
        queue.set_timeout(Some("console.log('hi')".into()), 0);
        let js = queue.get_expired_timer_callbacks_js();
        assert!(js.contains("console.log('hi')"));
        assert!(js.starts_with("try {"));
    }

    #[test]
    fn test_get_expired_timer_callbacks_js_skips_nonzero_delay() {
        let mut queue = TimerQueue::new();
        queue.set_timeout(Some("slow()".into()), 5000);
        let js = queue.get_expired_timer_callbacks_js();
        assert!(js.is_empty());
    }

    #[test]
    fn test_get_expired_timer_callbacks_js_skips_none_callback() {
        let mut queue = TimerQueue::new();
        queue.set_timeout(None, 0);
        let js = queue.get_expired_timer_callbacks_js();
        assert!(js.is_empty());
    }

    #[test]
    fn test_get_expired_timer_callbacks_js_skips_fired() {
        let mut queue = TimerQueue::new();
        queue.set_timeout(Some("once()".into()), 0);
        queue.mark_delay_zero_fired();
        let js = queue.get_expired_timer_callbacks_js();
        assert!(js.is_empty());
    }

    #[test]
    fn test_get_expired_timer_callbacks_js_includes_delay_zero_intervals() {
        let mut queue = TimerQueue::new();
        queue.set_interval(Some("tick()".into()), 0);
        // delay=0 intervals ARE returned by get_expired_timer_callbacks_js
        // (only mark_delay_zero_fired skips them)
        let js = queue.get_expired_timer_callbacks_js();
        assert!(js.contains("tick()"));
    }

    #[test]
    fn test_get_expired_timer_callbacks_js_multiple() {
        let mut queue = TimerQueue::new();
        queue.set_timeout(Some("a()".into()), 0);
        queue.set_timeout(Some("b()".into()), 0);
        queue.set_timeout(Some("c()".into()), 100); // skipped
        let js = queue.get_expired_timer_callbacks_js();
        assert!(js.contains("a()"));
        assert!(js.contains("b()"));
        assert!(!js.contains("c()"));
    }

    #[test]
    fn test_mark_delay_zero_fired_increments_tick_count() {
        let mut queue = TimerQueue::new();
        queue.set_timeout(Some("a".into()), 0);
        queue.set_timeout(Some("b".into()), 0);
        queue.set_timeout(Some("c".into()), 100); // not delay=0
        assert_eq!(queue.tick_count(), 0);
        queue.mark_delay_zero_fired();
        assert_eq!(queue.tick_count(), 2);
    }

    #[test]
    fn test_mark_delay_zero_fired_idempotent() {
        let mut queue = TimerQueue::new();
        queue.set_timeout(Some("a".into()), 0);
        queue.mark_delay_zero_fired();
        assert_eq!(queue.tick_count(), 1);
        queue.mark_delay_zero_fired();
        assert_eq!(queue.tick_count(), 1); // already fired
    }

    #[test]
    fn test_mark_delay_zero_does_not_fire_intervals() {
        let mut queue = TimerQueue::new();
        queue.set_interval(Some("tick".into()), 0);
        queue.mark_delay_zero_fired();
        assert_eq!(queue.tick_count(), 0);
        assert!(!queue.timers[0].is_fired);
    }

    #[test]
    fn test_is_at_limit() {
        let mut queue = TimerQueue::new();
        assert!(!queue.is_at_limit());
        // Force tick count to max
        for _ in 0..queue.max_ticks {
            queue.tick_count += 1;
        }
        assert!(queue.is_at_limit());
    }

    #[test]
    fn test_timer_entry_defaults() {
        let mut queue = TimerQueue::new();
        let id = queue.set_timeout(Some("cb".into()), 1000);
        let entry = queue.timers.iter().find(|t| t.id == id).unwrap();
        assert_eq!(entry.id, id);
        assert_eq!(entry.callback_str, Some("cb".into()));
        assert_eq!(entry.delay_ms, 1000);
        assert!(!entry.is_interval);
        assert!(!entry.is_fired);
    }
}
