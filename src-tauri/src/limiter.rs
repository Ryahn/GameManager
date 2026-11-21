//! A small sliding-window rate limiter with priorities and a 429 circuit.
//!
//! Used for every outbound request family (catalog API, cover images, and later
//! F95 thread pages and file hosts) so all pacing rules live in one place and
//! can be tested with a paused tokio clock.
//!
//! Rules:
//! - At most `capacity` permits per `window`.
//! - Background work may only use `background_share` of the window so a user
//!   action never waits behind a background sync.
//! - Optional minimum gap (+ random jitter) between two permits.
//! - After the server says "slow down" (429), *every* request fails fast until
//!   the cooldown ends. Sending more requests while rate limited only extends
//!   the ban, so we never queue them up behind it.

use std::collections::VecDeque;
use std::time::Duration;

use rand::Rng;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Foreground,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LimitError {
    #[error("rate limited, try again in {0:?}")]
    CoolingDown(Duration),
}

#[derive(Debug, Clone)]
pub struct LimiterConfig {
    pub capacity: u32,
    pub window: Duration,
    /// Fraction (0..=1) of `capacity` that background work may use.
    pub background_share: f32,
    pub min_gap: Duration,
    pub jitter: Duration,
    /// Cooldown used when the server gives no Retry-After.
    pub default_cooldown: Duration,
    /// Cooldowns double on repeated strikes up to this ceiling.
    pub max_cooldown: Duration,
}

impl LimiterConfig {
    pub fn per_minute(capacity: u32) -> Self {
        Self {
            capacity,
            window: Duration::from_secs(60),
            background_share: 0.75,
            min_gap: Duration::ZERO,
            jitter: Duration::ZERO,
            default_cooldown: Duration::from_secs(60),
            max_cooldown: Duration::from_secs(15 * 60),
        }
    }

    pub fn per_second(capacity: u32) -> Self {
        Self {
            window: Duration::from_secs(1),
            ..Self::per_minute(capacity)
        }
    }
}

#[derive(Debug)]
struct State {
    sent: VecDeque<Instant>,
    last: Option<Instant>,
    next_gap: Duration,
    cooldown_until: Option<Instant>,
    strikes: u32,
}

#[derive(Debug)]
pub struct RateLimiter {
    cfg: LimiterConfig,
    state: Mutex<State>,
}

impl RateLimiter {
    pub fn new(cfg: LimiterConfig) -> Self {
        assert!(cfg.capacity > 0, "limiter capacity must be > 0");
        Self {
            state: Mutex::new(State {
                sent: VecDeque::new(),
                last: None,
                next_gap: cfg.min_gap,
                cooldown_until: None,
                strikes: 0,
            }),
            cfg,
        }
    }

    fn cap_for(&self, p: Priority) -> usize {
        match p {
            Priority::Foreground => self.cfg.capacity as usize,
            Priority::Background => {
                ((self.cfg.capacity as f32 * self.cfg.background_share).floor() as usize).max(1)
            }
        }
    }

    fn pick_gap(&self) -> Duration {
        if self.cfg.jitter.is_zero() {
            return self.cfg.min_gap;
        }
        let j = rand::thread_rng().gen_range(0..=self.cfg.jitter.as_millis() as u64);
        self.cfg.min_gap + Duration::from_millis(j)
    }

    /// Wait for a permit. Fails fast while a 429 cooldown is active.
    pub async fn acquire(&self, priority: Priority) -> Result<(), LimitError> {
        loop {
            let wait = {
                let mut s = self.state.lock().await;
                let now = Instant::now();

                if let Some(until) = s.cooldown_until {
                    if until > now {
                        return Err(LimitError::CoolingDown(until - now));
                    }
                    s.cooldown_until = None;
                }

                while let Some(front) = s.sent.front() {
                    if now.duration_since(*front) >= self.cfg.window {
                        s.sent.pop_front();
                    } else {
                        break;
                    }
                }

                let gap_wait = s
                    .last
                    .map(|l| (l + s.next_gap).saturating_duration_since(now))
                    .unwrap_or(Duration::ZERO);

                let cap = self.cap_for(priority);
                let window_wait = if s.sent.len() < cap {
                    Duration::ZERO
                } else {
                    // The permit that must expire before we fit under `cap`.
                    let idx = s.sent.len() - cap;
                    (s.sent[idx] + self.cfg.window).saturating_duration_since(now)
                };

                let wait = gap_wait.max(window_wait);
                if wait.is_zero() {
                    s.sent.push_back(now);
                    s.last = Some(now);
                    s.next_gap = self.pick_gap();
                    return Ok(());
                }
                wait
            };
            sleep(wait).await;
        }
    }

    /// Record a "too many requests" answer. Returns the cooldown applied.
    pub async fn report_rate_limited(&self, retry_after: Option<Duration>) -> Duration {
        let mut s = self.state.lock().await;
        let backoff = self
            .cfg
            .default_cooldown
            .saturating_mul(1u32 << s.strikes.min(8))
            .min(self.cfg.max_cooldown);
        let cooldown = retry_after.map(|r| r.max(backoff)).unwrap_or(backoff);
        s.strikes = s.strikes.saturating_add(1);
        s.cooldown_until = Some(Instant::now() + cooldown);
        cooldown
    }

    /// Record a normal answer; resets the backoff ladder.
    pub async fn report_success(&self) {
        let mut s = self.state.lock().await;
        s.strikes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn allows_capacity_then_waits_for_window() {
        let l = RateLimiter::new(LimiterConfig::per_minute(90));
        let start = Instant::now();
        for _ in 0..90 {
            l.acquire(Priority::Foreground).await.unwrap();
        }
        assert_eq!(Instant::now(), start, "first 90 permits are immediate");
        l.acquire(Priority::Foreground).await.unwrap();
        assert!(Instant::now() - start >= Duration::from_secs(60));
    }

    #[tokio::test(start_paused = true)]
    async fn background_leaves_headroom_for_foreground() {
        let l = RateLimiter::new(LimiterConfig::per_minute(8)); // background cap = 6
        let start = Instant::now();
        for _ in 0..6 {
            l.acquire(Priority::Background).await.unwrap();
        }
        // Foreground still gets the reserved slots instantly.
        l.acquire(Priority::Foreground).await.unwrap();
        l.acquire(Priority::Foreground).await.unwrap();
        assert_eq!(Instant::now(), start);
        // Background must wait for the window to roll.
        l.acquire(Priority::Background).await.unwrap();
        assert!(Instant::now() - start >= Duration::from_secs(60));
    }

    #[tokio::test(start_paused = true)]
    async fn min_gap_with_jitter_is_respected() {
        let l = RateLimiter::new(LimiterConfig {
            min_gap: Duration::from_secs(5),
            jitter: Duration::from_secs(2),
            ..LimiterConfig::per_minute(100)
        });
        let mut prev = None;
        for _ in 0..5 {
            l.acquire(Priority::Foreground).await.unwrap();
            let now = Instant::now();
            if let Some(p) = prev {
                let gap: Duration = now - p;
                assert!(
                    gap >= Duration::from_secs(5) && gap <= Duration::from_secs(7),
                    "{gap:?}"
                );
            }
            prev = Some(now);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn cooldown_fails_fast_and_doubles() {
        let l = RateLimiter::new(LimiterConfig::per_minute(90));
        let c1 = l.report_rate_limited(None).await;
        assert_eq!(c1, Duration::from_secs(60));
        assert!(matches!(
            l.acquire(Priority::Foreground).await,
            Err(LimitError::CoolingDown(_))
        ));
        tokio::time::advance(Duration::from_secs(61)).await;
        l.acquire(Priority::Foreground).await.unwrap();

        let c2 = l.report_rate_limited(None).await;
        assert_eq!(c2, Duration::from_secs(120), "second strike doubles");
        l.report_success().await;
        tokio::time::advance(Duration::from_secs(121)).await;
        let c3 = l.report_rate_limited(None).await;
        assert_eq!(c3, Duration::from_secs(60), "success resets the ladder");
    }

    #[tokio::test(start_paused = true)]
    async fn retry_after_is_honoured_and_capped() {
        let l = RateLimiter::new(LimiterConfig::per_minute(90));
        let c = l.report_rate_limited(Some(Duration::from_secs(300))).await;
        assert_eq!(c, Duration::from_secs(300));
        let mut strikes = Duration::ZERO;
        for _ in 0..10 {
            strikes = l.report_rate_limited(None).await;
        }
        assert_eq!(strikes, Duration::from_secs(15 * 60));
    }
}
