use rgsl::rng::algorithms::mt19937;
use rgsl::rng::Rng as GslRng;
use std::ops::{Bound, RangeBounds};

/// simple wrapper around GSL mt19937
/// implementing just the parts of rand::Rng I use
pub struct MT19937(GslRng);

trait UsizeConvertible {
    fn as_usize(self) -> usize;
    fn from_usize(n: usize) -> Self;
}

macro_rules! gen_usize_convertible {
    ($Int:ident) => {
        impl UsizeConvertible for $Int {
            #[inline]
            fn as_usize(self) -> usize {
                self as usize
            }

            #[inline]
            fn from_usize(n: usize) -> Self {
                n as $Int
            }
        }
    };
}

gen_usize_convertible!(usize);
gen_usize_convertible!(u64);
gen_usize_convertible!(u32);
gen_usize_convertible!(u16);
gen_usize_convertible!(u8);

impl MT19937 {
    /// semantics differ from rand::SeedableRng::seed_from_u64
    pub fn seed_from_u64(seed: u64) -> Self {
        let mut rng = GslRng::new(mt19937()).unwrap();
        rng.set(seed as usize);
        Self(rng)
    }

    #[allow(private_bounds)]
    pub fn gen_range<N: UsizeConvertible + Copy>(&mut self, range: impl RangeBounds<N>) -> N {
        let start = match range.start_bound() {
            Bound::Included(x) => x.as_usize(),
            Bound::Excluded(x) => x.as_usize() + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(x) => x.as_usize() + 1,
            Bound::Excluded(x) => x.as_usize(),
            Bound::Unbounded => panic!(),
        };

        N::from_usize(self.0.uniform_int(end - start) + start)
    }
    pub fn gen_bool(&mut self, p: f64) -> bool {
        self.0.uniform() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range() {
        let mut rng = MT19937::seed_from_u64(4);
        for x in 1..100u64 {
            let r = rng.gen_range(0..x);
            assert!(r < x, "{} < {}", r, x);
            let r = rng.gen_range(x..100);
            assert!(r >= x, "{} ≥ {}", r, x);
        }
    }
}
