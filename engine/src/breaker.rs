//! The ysob: a circuit breaker. The same failure three times in a row and
//! the pirate is held until the captain decides.

pub const STRIKES: u32 = 3;

#[derive(Debug, Default)]
pub struct Breaker {
    last: Option<String>,
    count: u32,
    pub held: bool,
}

/// Reduce an error to its shape, so "line 41" and "line 42" count as the same failure.
pub fn signature(tool: &str, error: &str) -> String {
    let mut out = String::with_capacity(180);
    out.push_str(tool);
    out.push(':');
    let mut last_hash = false;
    for c in error.chars().take(400) {
        if c.is_ascii_digit() {
            if !last_hash {
                out.push('#');
            }
            last_hash = true;
        } else {
            last_hash = false;
            out.extend(c.to_lowercase());
        }
        if out.len() >= 180 {
            break;
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Breaker {
    /// Record a failed tool call. Returns true the moment the pirate becomes held.
    pub fn fail(&mut self, tool: &str, error: &str) -> bool {
        let sig = signature(tool, error);
        if self.last.as_deref() == Some(sig.as_str()) {
            self.count += 1;
        } else {
            self.last = Some(sig);
            self.count = 1;
        }
        if self.count >= STRIKES && !self.held {
            self.held = true;
            return true;
        }
        false
    }
    pub fn ok(&mut self) {
        self.last = None;
        self.count = 0;
    }
    pub fn release(&mut self) {
        self.held = false;
        self.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_identical_failures_hold_the_pirate() {
        let mut b = Breaker::default();
        assert!(!b.fail("Bash", "error: column email already exists"));
        assert!(!b.fail("Bash", "error: column email already exists"));
        assert!(b.fail("Bash", "error: column email already exists"));
        assert!(b.held);
        assert!(!b.fail("Bash", "error: column email already exists"), "only trips once");
    }

    #[test]
    fn a_success_or_a_different_failure_resets_the_count() {
        let mut b = Breaker::default();
        b.fail("Bash", "boom");
        b.fail("Bash", "boom");
        b.ok();
        assert!(!b.fail("Bash", "boom"));
        b.fail("Bash", "boom");
        assert!(!b.fail("Bash", "a different failure"));
        assert!(!b.held);
    }

    #[test]
    fn numbers_do_not_hide_the_same_failure() {
        assert_eq!(signature("Bash", "test failed at line 41 after 1203ms"), signature("Bash", "test failed at line 7 after 88ms"));
        assert_ne!(signature("Bash", "x"), signature("Edit", "x"));
    }

    #[test]
    fn release_gives_a_clean_slate() {
        let mut b = Breaker::default();
        for _ in 0..3 {
            b.fail("Bash", "boom");
        }
        b.release();
        assert!(!b.held);
        assert!(!b.fail("Bash", "boom"));
    }
}
