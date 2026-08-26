use crate::random::Random;

/// Difference-updatable Zobrist table for `position x value` states.
pub struct Zobrist {
    positions: usize,
    values: usize,
    table: Vec<u64>,
}

impl Zobrist {
    pub fn new(
        positions: usize,
        values: usize,
        seed: u64,
    ) -> Self {
        assert!(values > 0, "zobrist needs at least one value");
        let table_len = positions
            .checked_mul(values)
            .expect("zobrist table size overflow");
        let mut random = Random::new(seed);
        let mut table = Vec::with_capacity(table_len);
        for _ in 0..table_len {
            table.push(random.next_u64());
        }
        Self {
            positions,
            values,
            table,
        }
    }

    pub const fn positions(&self) -> usize {
        self.positions
    }
    pub const fn values(&self) -> usize {
        self.values
    }

    #[inline]
    pub fn key(
        &self,
        position: usize,
        value: usize,
    ) -> u64 {
        assert!(
            position < self.positions && value < self.values,
            "zobrist index out of range"
        );
        self.table[position * self.values + value]
    }

    pub fn hash_values(
        &self,
        values: &[usize],
    ) -> u64 {
        assert_eq!(values.len(), self.positions, "wrong state length");
        values
            .iter()
            .enumerate()
            .fold(0, |hash, (position, &value)| {
                hash ^ self.key(position, value)
            })
    }

    #[inline]
    pub fn update(
        &self,
        hash: u64,
        position: usize,
        old_value: usize,
        new_value: usize,
    ) -> u64 {
        hash ^ self.key(position, old_value) ^ self.key(position, new_value)
    }
}

#[cfg(test)]
mod tests {
    use super::Zobrist;

    #[test]
    fn incremental_update_matches_full_hash() {
        let z = Zobrist::new(3, 4, 12);
        let before = [0, 1, 2];
        let after = [0, 3, 2];
        assert_eq!(
            z.update(z.hash_values(&before), 1, 1, 3),
            z.hash_values(&after)
        );
    }

    #[test]
    #[should_panic(expected = "zobrist table size overflow")]
    fn rejects_table_size_overflow() {
        let _ = Zobrist::new(usize::MAX, 2, 1);
    }
}
