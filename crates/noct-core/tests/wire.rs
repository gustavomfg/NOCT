//! `JSON.stringify` number text, checked against the values Node.js writes.

use noct_core::wire::js_number_string;

#[test]
fn numbers_are_written_like_json_stringify() {
    let cases: &[(f64, &str)] = &[
        (2.0, "2"),
        (0.5, "0.5"),
        (-2.5, "-2.5"),
        (0.0, "0"),
        (-0.0, "0"),
        (100.0, "100"),
        (0.000001, "0.000001"),
        (0.0000001, "1e-7"),
        (1.5e-7, "1.5e-7"),
        (1e21, "1e+21"),
        (1.5e21, "1.5e+21"),
        (123456789.125, "123456789.125"),
        (0.1 + 0.2, "0.30000000000000004"),
    ];
    for (value, expected) in cases {
        assert_eq!(js_number_string(*value), *expected, "{value:e}");
    }
    assert_eq!(js_number_string(f64::NAN), "null");
    assert_eq!(js_number_string(f64::INFINITY), "null");
}
