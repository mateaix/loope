use t::multiply;

#[test]
fn saturates_on_overflow() {
    assert_eq!(multiply(3, 4), 12);
    assert_eq!(multiply(i64::MAX, 2), i64::MAX);
    assert_eq!(multiply(i64::MIN, 2), i64::MIN);
}
