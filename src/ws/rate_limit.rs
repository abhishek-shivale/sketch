use tokio::time::Instant;

/// Classic token bucket: `burst` tokens, refilled at `rate` per second.
#[derive(Debug)]
pub struct TokenBucket {
    capacity: f64,
    tokens: f64,
    per_sec: f64,
    last: Instant,
}

impl TokenBucket {
    pub fn new(rate_per_sec: u32, burst: u32) -> Self {
        Self {
            capacity: f64::from(burst),
            tokens: f64::from(burst),
            per_sec: f64::from(rate_per_sec),
            last: Instant::now(),
        }
    }

    pub fn try_acquire(&mut self) -> bool {
        self.try_acquire_at(Instant::now())
    }

    fn try_acquire_at(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.per_sec).min(self.capacity);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn allows_burst_then_refills() {
        let mut bucket = TokenBucket::new(10, 3);
        let t0 = bucket.last;
        assert!((0..3).all(|_| bucket.try_acquire_at(t0)));
        assert!(!bucket.try_acquire_at(t0));

        // 100ms at 10/s refills exactly one token.
        let t1 = t0 + Duration::from_millis(100);
        assert!(bucket.try_acquire_at(t1));
        assert!(!bucket.try_acquire_at(t1));
    }

    #[test]
    fn never_exceeds_capacity() {
        let mut bucket = TokenBucket::new(1000, 2);
        let later = bucket.last + Duration::from_secs(60);
        assert!(bucket.try_acquire_at(later));
        assert!(bucket.try_acquire_at(later));
        assert!(!bucket.try_acquire_at(later));
    }
}
