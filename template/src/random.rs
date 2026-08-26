use std::ops::Range;

/// Fast deterministic xorshift random generator for heuristic search.
pub struct Random {
    state: u64,
}

impl Random {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                seed
            },
        }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    #[inline]
    pub fn usize(&mut self, range: Range<usize>) -> usize {
        assert!(range.start < range.end, "random range must not be empty");
        range.start + self.next_u64() as usize % (range.end - range.start)
    }

    #[inline]
    pub fn f64(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / ((1u64 << 53) as f64);
        ((self.next_u64() >> 11) as f64) * SCALE
    }

    #[inline]
    pub fn bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::Random;

    #[test]
    fn deterministic_for_same_seed() {
        let mut a = Random::new(123);
        let mut b = Random::new(123);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn generated_values_are_in_range() {
        let mut random = Random::new(1);
        for _ in 0..10_000 {
            assert!((7..19).contains(&random.usize(7..19)));
            let x = random.f64();
            assert!((0.0..1.0).contains(&x));
        }
    }
}
