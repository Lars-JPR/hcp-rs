#[link(name = "m")]
extern "C" {
    fn lgamma(x: f64) -> f64;
}

pub fn ln_fact(x: usize) -> f64 {
    unsafe { lgamma((x + 1) as f64) }
}

/// TODO: this may precompute ln-factorials in the future
pub fn init() {}
