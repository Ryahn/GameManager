//! Anti-ban pacing for file hosts, and the mass-download policy.
//!
//! Users can make these stricter in Settings but never looser: every value
//! passes through the hard caps below.

use std::collections::HashMap;
use std::time::Duration;

use rand::Rng;

/// Downloads running at once, across all hosts.
pub const DEFAULT_ACTIVE: usize = 2;
pub const MAX_ACTIVE: usize = 3;
// Downloads running at once from one host: always one (see the scheduler).
/// Gap between two download starts on the same host: 45 s ± 15 s.
pub const HOST_GAP: Duration = Duration::from_secs(45);
pub const HOST_GAP_JITTER: Duration = Duration::from_secs(15);
/// No setting or jitter can bring the gap below this.
pub const HOST_GAP_FLOOR: Duration = Duration::from_secs(20);
/// After a host says "too many" (429/403/quota): 15 min, doubling to 2 h.
pub const HOST_COOLDOWN: Duration = Duration::from_secs(15 * 60);
pub const HOST_COOLDOWN_MAX: Duration = Duration::from_secs(2 * 60 * 60);
/// Network errors retry after 30 s, 2 min, 10 min, then give up.
pub const RETRY_BACKOFF: [u64; 3] = [30, 120, 600];

/// Queueing this many at once shows the mass-download warning.
pub const WARN_BATCH: usize = 5;
/// More than this many starts in 24 hours needs a typed confirmation.
pub const DAILY_SOFT_LIMIT: usize = 15;
pub const CONFIRM_PHRASE: &str = "download anyway";

pub fn clamp_active(v: usize) -> usize {
    v.clamp(1, MAX_ACTIVE)
}

#[derive(Debug, Default)]
struct HostState {
    next_start: u64,
    cooldown_until: u64,
    strikes: u32,
}

/// Per-host pacing. Times are unix seconds so the logic is easy to test.
#[derive(Debug, Default)]
pub struct HostGates {
    hosts: HashMap<String, HostState>,
}

impl HostGates {
    /// `Ok` if a download from `host` may start at `now`, otherwise the
    /// unix time it may start.
    pub fn check(&self, host: &str, now: u64) -> Result<(), u64> {
        match self.hosts.get(host) {
            Some(h) => {
                let at = h.next_start.max(h.cooldown_until);
                if now >= at { Ok(()) } else { Err(at) }
            }
            None => Ok(()),
        }
    }

    /// Records a start and schedules the earliest next one.
    pub fn started(&mut self, host: &str, now: u64) {
        let jitter = rand::thread_rng().gen_range(0..=HOST_GAP_JITTER.as_secs() * 2);
        let gap = (HOST_GAP.as_secs() - HOST_GAP_JITTER.as_secs() + jitter).max(HOST_GAP_FLOOR.as_secs());
        let h = self.hosts.entry(host.to_string()).or_default();
        h.next_start = now + gap;
    }

    /// The host pushed back. Returns how long it is now blocked.
    pub fn penalize(&mut self, host: &str, now: u64) -> Duration {
        let h = self.hosts.entry(host.to_string()).or_default();
        let secs = (HOST_COOLDOWN.as_secs() << h.strikes.min(4)).min(HOST_COOLDOWN_MAX.as_secs());
        h.strikes += 1;
        h.cooldown_until = now + secs;
        Duration::from_secs(secs)
    }

    /// A clean download resets the strike count.
    pub fn succeeded(&mut self, host: &str) {
        if let Some(h) = self.hosts.get_mut(host) {
            h.strikes = 0;
        }
    }

    pub fn cooldown_until(&self, host: &str) -> Option<u64> {
        self.hosts.get(host).map(|h| h.cooldown_until).filter(|t| *t > 0)
    }
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct Preflight {
    /// Jobs already waiting.
    pub queued: usize,
    /// Jobs this action adds.
    pub adding: usize,
    /// Downloads started in the last 24 hours.
    pub daily: usize,
    pub warn: bool,
    pub needs_confirm: bool,
    /// Rough time for the whole queue at the current pacing.
    pub eta_secs: u64,
}

/// Decides whether to warn before queueing `adding` more downloads.
pub fn preflight(queued_per_host: &HashMap<String, usize>, adding_hosts: &[String], daily: usize) -> Preflight {
    let queued: usize = queued_per_host.values().sum();
    let adding = adding_hosts.len();
    let mut per_host = queued_per_host.clone();
    for h in adding_hosts {
        *per_host.entry(h.clone()).or_default() += 1;
    }
    let busiest = per_host.values().copied().max().unwrap_or(0) as u64;
    Preflight {
        queued,
        adding,
        daily,
        warn: adding >= WARN_BATCH || queued + adding >= WARN_BATCH,
        needs_confirm: daily + adding > DAILY_SOFT_LIMIT,
        eta_secs: busiest.saturating_sub(1) * HOST_GAP.as_secs(),
    }
}

pub fn confirmed(phrase: Option<&str>) -> bool {
    phrase.is_some_and(|p| p.trim().eq_ignore_ascii_case(CONFIRM_PHRASE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_gap_stays_within_bounds() {
        for _ in 0..200 {
            let mut g = HostGates::default();
            g.started("pixeldrain.com", 1000);
            let next = g.check("pixeldrain.com", 1000).unwrap_err();
            let gap = next - 1000;
            assert!((30..=60).contains(&gap), "gap {gap}");
            assert!(gap >= HOST_GAP_FLOOR.as_secs());
            assert!(g.check("other.host", 1000).is_ok(), "hosts are independent");
            assert!(g.check("pixeldrain.com", next).is_ok());
        }
    }

    #[test]
    fn cooldown_doubles_and_caps() {
        let mut g = HostGates::default();
        let mins: Vec<u64> = (0..6).map(|_| g.penalize("h", 0).as_secs() / 60).collect();
        assert_eq!(mins, vec![15, 30, 60, 120, 120, 120]);
        assert_eq!(g.check("h", 0), Err(7200));
        g.succeeded("h");
        assert_eq!(g.penalize("h", 0).as_secs(), 15 * 60);
    }

    #[test]
    fn warns_and_asks_for_confirmation() {
        let none = HashMap::new();
        let p = preflight(&none, &["a".into()], 0);
        assert!(!p.warn && !p.needs_confirm);

        let hosts: Vec<String> = (0..5).map(|_| "pixeldrain.com".to_string()).collect();
        let p = preflight(&none, &hosts, 0);
        assert!(p.warn);
        assert_eq!(p.eta_secs, 4 * 45);

        let mut queued = HashMap::new();
        queued.insert("a".to_string(), 4);
        assert!(preflight(&queued, &["b".into()], 0).warn);

        assert!(!preflight(&none, &["a".into()], 14).needs_confirm);
        assert!(preflight(&none, &["a".into()], 15).needs_confirm);
    }

    #[test]
    fn confirmation_phrase_and_caps() {
        assert!(confirmed(Some("  Download Anyway ")));
        assert!(!confirmed(Some("yes")));
        assert!(!confirmed(None));
        assert_eq!(clamp_active(10), MAX_ACTIVE);
        assert_eq!(clamp_active(0), 1);
    }
}
