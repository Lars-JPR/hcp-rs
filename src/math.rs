static mut _LN_FACT: Vec<f64> = Vec::new();

#[link(name = "m")]
extern "C" {
    fn lgamma(x: f64) -> f64;
}

fn _ln_fact(x: usize) -> f64 {
    unsafe { lgamma((x + 1) as f64) }
}

/// `ln(x!)`. precompute_ln_fact(m) *must* be called with m ≥ x before calling this.
#[inline]
pub fn ln_fact(x: usize) -> f64 {
    unsafe { _LN_FACT[x] }
}

/// precompute ln-factorials. This *must* be called before using ln_fact()
pub fn precompute_ln_fact(m: usize) {
    let required_len = m + 1;
    let current_len = unsafe { _LN_FACT.len() };
    if required_len <= current_len {
        return;
    }
    unsafe {
        _LN_FACT.reserve_exact(required_len - current_len);
        _LN_FACT.splice(
            current_len..current_len,
            (current_len..required_len).map(_ln_fact),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    macro_rules! assert_close {
        ($left:expr, $right:expr $(,)?) => {
            let err = 0.001;
            assert!(
                ($left - $right).abs() < err,
                "{}, {} differ by more than {}",
                $left,
                $right,
                err
            );
        };
    }
    #[test]
    fn test_ln_fact() {
        precompute_ln_fact(100);
        assert_eq!(ln_fact(0), 0.0);
        assert_close!(ln_fact(2), 0.6931);
        assert_close!(ln_fact(10), 363.7394);
    }
}
