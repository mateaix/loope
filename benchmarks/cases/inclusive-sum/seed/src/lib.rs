/// Buggy: off-by-one — the exclusive range omits n.
pub fn sum_to(n: u64) -> u64 {
    (1..n).sum()
}
