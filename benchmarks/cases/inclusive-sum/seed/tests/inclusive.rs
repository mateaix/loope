use t::sum_to;

#[test]
fn inclusive() {
    assert_eq!(sum_to(5), 15);
    assert_eq!(sum_to(1), 1);
}
