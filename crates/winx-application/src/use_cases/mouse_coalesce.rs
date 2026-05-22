//! Agrega deltas de mouse antes de enviar no stream Input.

#[derive(Debug, Default)]
pub struct MouseCoalescer {
    pending_dx: i32,
    pending_dy: i32,
}

impl MouseCoalescer {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending_dx: 0,
            pending_dy: 0,
        }
    }

    pub fn push(&mut self, dx: i32, dy: i32) {
        self.pending_dx = self.pending_dx.wrapping_add(dx);
        self.pending_dy = self.pending_dy.wrapping_add(dy);
    }

    #[must_use]
    pub fn take_if_significant(&mut self, min_manhattan: i32) -> Option<(i32, i32)> {
        if self.pending_dx.abs().saturating_add(self.pending_dy.abs()) < min_manhattan {
            return None;
        }
        let v = (self.pending_dx, self.pending_dy);
        self.pending_dx = 0;
        self.pending_dy = 0;
        Some(v)
    }

    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending_dx != 0 || self.pending_dy != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_deltas() {
        let mut c = MouseCoalescer::new();
        c.push(1, 1);
        c.push(2, 2);
        let (dx, dy) = c.take_if_significant(2).expect("significant");
        assert_eq!((dx, dy), (3, 3));
    }

    #[test]
    fn below_threshold_returns_none() {
        let mut c = MouseCoalescer::new();
        c.push(0, 1);
        assert!(c.take_if_significant(2).is_none());
        assert!(c.has_pending());
    }

    #[test]
    fn take_clears_pending() {
        let mut c = MouseCoalescer::new();
        c.push(5, 5);
        assert!(c.take_if_significant(2).is_some());
        assert!(!c.has_pending());
        assert!(c.take_if_significant(2).is_none());
    }
}
