/// Buggy: panics (divide by zero) on an empty slice.
pub fn average(xs: &[i64]) -> i64 {
    xs.iter().sum::<i64>() / xs.len() as i64
}
