/// Buggy on purpose: `a * b` overflows. The task is to make it saturate.
pub fn multiply(a: i64, b: i64) -> i64 {
    a * b
}
