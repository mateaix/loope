use t::average;

#[test]
fn handles_empty() {
    assert_eq!(average(&[2, 4, 6]), 4);
    assert_eq!(average(&[]), 0);
}
