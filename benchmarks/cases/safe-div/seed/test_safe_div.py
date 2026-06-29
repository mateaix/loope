from safe_div import safe_div


def test_safe_div():
    assert safe_div(10, 2) == 5
    assert safe_div(10, 0) == 0
