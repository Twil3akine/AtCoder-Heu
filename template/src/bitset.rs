use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl, Shr};

/// A small fixed-size bitset for boards larger than `u64`/`u128`.
///
/// Prefer raw integers when they are sufficient. This type intentionally has no
/// dynamic allocation or broad collection API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BitSet<const WORDS: usize> {
    words: [u64; WORDS],
}

impl<const WORDS: usize> Default for BitSet<WORDS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const WORDS: usize> BitSet<WORDS> {
    pub const fn new() -> Self {
        Self { words: [0; WORDS] }
    }

    pub const fn from_words(words: [u64; WORDS]) -> Self {
        Self { words }
    }

    pub const fn words(&self) -> &[u64; WORDS] {
        &self.words
    }

    #[inline]
    pub const fn bit_len() -> usize {
        WORDS * 64
    }

    #[inline]
    pub fn contains(
        &self,
        index: usize,
    ) -> bool {
        self.assert_index(index);
        self.words[index / 64] & (1u64 << (index % 64)) != 0
    }

    #[inline]
    pub fn insert(
        &mut self,
        index: usize,
    ) -> bool {
        self.assert_index(index);
        let word = &mut self.words[index / 64];
        let bit = 1u64 << (index % 64);
        let old = *word & bit != 0;
        *word |= bit;
        !old
    }

    #[inline]
    pub fn remove(
        &mut self,
        index: usize,
    ) -> bool {
        self.assert_index(index);
        let word = &mut self.words[index / 64];
        let bit = 1u64 << (index % 64);
        let old = *word & bit != 0;
        *word &= !bit;
        old
    }

    #[inline]
    pub fn toggle(
        &mut self,
        index: usize,
    ) {
        self.assert_index(index);
        self.words[index / 64] ^= 1u64 << (index % 64);
    }

    #[inline]
    pub fn clear_all(&mut self) {
        self.words = [0; WORDS];
    }

    #[inline]
    pub fn count_ones(&self) -> usize {
        self.words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    #[inline]
    pub fn intersects(
        &self,
        other: &Self,
    ) -> bool {
        self.words
            .iter()
            .zip(
                other
                    .words
                    .iter(),
            )
            .any(|(a, b)| *a & *b != 0)
    }

    #[inline]
    pub fn is_disjoint(
        &self,
        other: &Self,
    ) -> bool {
        !self.intersects(other)
    }

    #[inline]
    pub fn difference(
        &self,
        other: &Self,
    ) -> Self {
        let mut result = Self::new();
        for i in 0..WORDS {
            result.words[i] = self.words[i] & !other.words[i];
        }
        result
    }

    #[inline]
    fn assert_index(
        &self,
        index: usize,
    ) {
        assert!(index < Self::bit_len(), "bit index out of range");
    }

    fn shifted_left(
        &self,
        amount: usize,
    ) -> Self {
        if amount >= Self::bit_len() {
            return Self::new();
        }
        let word_shift = amount / 64;
        let bit_shift = amount % 64;
        let mut result = Self::new();
        for dst in (word_shift..WORDS).rev() {
            let src = dst - word_shift;
            result.words[dst] |= self.words[src] << bit_shift;
            if bit_shift != 0 && src > 0 {
                result.words[dst] |= self.words[src - 1] >> (64 - bit_shift);
            }
        }
        result
    }

    fn shifted_right(
        &self,
        amount: usize,
    ) -> Self {
        if amount >= Self::bit_len() {
            return Self::new();
        }
        let word_shift = amount / 64;
        let bit_shift = amount % 64;
        let mut result = Self::new();
        for dst in 0..(WORDS - word_shift) {
            let src = dst + word_shift;
            result.words[dst] |= self.words[src] >> bit_shift;
            if bit_shift != 0 && src + 1 < WORDS {
                result.words[dst] |= self.words[src + 1] << (64 - bit_shift);
            }
        }
        result
    }
}

impl<const WORDS: usize> BitAnd for BitSet<WORDS> {
    type Output = Self;
    fn bitand(
        mut self,
        rhs: Self,
    ) -> Self {
        self &= rhs;
        self
    }
}
impl<const WORDS: usize> BitAndAssign for BitSet<WORDS> {
    fn bitand_assign(
        &mut self,
        rhs: Self,
    ) {
        for i in 0..WORDS {
            self.words[i] &= rhs.words[i];
        }
    }
}
impl<const WORDS: usize> BitOr for BitSet<WORDS> {
    type Output = Self;
    fn bitor(
        mut self,
        rhs: Self,
    ) -> Self {
        self |= rhs;
        self
    }
}
impl<const WORDS: usize> BitOrAssign for BitSet<WORDS> {
    fn bitor_assign(
        &mut self,
        rhs: Self,
    ) {
        for i in 0..WORDS {
            self.words[i] |= rhs.words[i];
        }
    }
}
impl<const WORDS: usize> BitXor for BitSet<WORDS> {
    type Output = Self;
    fn bitxor(
        mut self,
        rhs: Self,
    ) -> Self {
        self ^= rhs;
        self
    }
}
impl<const WORDS: usize> BitXorAssign for BitSet<WORDS> {
    fn bitxor_assign(
        &mut self,
        rhs: Self,
    ) {
        for i in 0..WORDS {
            self.words[i] ^= rhs.words[i];
        }
    }
}
impl<const WORDS: usize> Not for BitSet<WORDS> {
    type Output = Self;
    fn not(mut self) -> Self {
        for word in &mut self.words {
            *word = !*word;
        }
        self
    }
}
impl<const WORDS: usize> Shl<usize> for BitSet<WORDS> {
    type Output = Self;
    fn shl(
        self,
        rhs: usize,
    ) -> Self {
        self.shifted_left(rhs)
    }
}
impl<const WORDS: usize> Shr<usize> for BitSet<WORDS> {
    type Output = Self;
    fn shr(
        self,
        rhs: usize,
    ) -> Self {
        self.shifted_right(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::BitSet;

    #[test]
    fn operations_difference_and_intersection_work() {
        let mut bits = BitSet::<2>::new();
        assert!(bits.insert(0));
        assert!(bits.insert(64));
        assert!(!bits.insert(64));
        assert_eq!(bits.count_ones(), 2);
        let other = BitSet::from_words([1, 1]);
        assert!(bits.intersects(&other));
        assert!(!bits.is_disjoint(&other));
        assert_eq!(
            bits.difference(&other)
                .count_ones(),
            0
        );
        assert_eq!((bits & other).words(), &[1, 1]);
        assert_eq!((bits ^ other).count_ones(), 0);
        assert_eq!((!bits).words(), &[!1, !1]);
        let union = bits | BitSet::from_words([0, 1 << 1]);
        assert_eq!(union.count_ones(), 3);
        assert!(bits.remove(0));
        assert!(bits.contains(64));
    }

    #[test]
    fn shifts_cross_word_boundaries_and_overshift_to_zero() {
        let mut bits = BitSet::<2>::new();
        bits.insert(0);
        bits.insert(63);
        bits.insert(64);
        bits.insert(127);
        assert!((bits << 1).contains(1));
        assert!((bits << 1).contains(64));
        assert!((bits << 1).contains(65));
        assert!(!(bits << 1).contains(0));
        assert!((bits >> 1).contains(62));
        assert!((bits >> 1).contains(63));
        assert!((bits >> 1).contains(126));
        assert!((bits << 63).contains(63));
        assert!((bits << 63).contains(126));
        assert!((bits << 64).contains(64));
        assert!((bits >> 64).contains(0));
        assert!((bits << 65).contains(65));
        assert!((bits >> 65).contains(62));
        assert_eq!((bits << 128).count_ones(), 0);
        assert_eq!((bits >> 128).count_ones(), 0);
    }
}
