//! Natural order of names.

use std::cmp::Ordering;

/// Compares `left` and `right` in natural order: letters without regard to
/// case, a run of digits by its value, whatever else by its bytes. Names
/// equal by that order are told apart by their bytes, so the order is total.
pub(crate) fn natural(left: &[u8], right: &[u8]) -> Ordering {
    let (mut rest_l, mut rest_r) = (left, right);
    while let (Some(&bl), Some(&br)) = (rest_l.first(), rest_r.first()) {
        let order = if bl.is_ascii_digit() && br.is_ascii_digit() {
            let (run_l, run_r) = (digits(rest_l), digits(rest_r));
            rest_l = &rest_l[run_l.len()..];
            rest_r = &rest_r[run_r.len()..];
            let (run_l, run_r) = (trim_zeros(run_l), trim_zeros(run_r));
            run_l.len().cmp(&run_r.len()).then_with(|| run_l.cmp(run_r))
        } else {
            rest_l = &rest_l[1..];
            rest_r = &rest_r[1..];
            bl.to_ascii_lowercase().cmp(&br.to_ascii_lowercase())
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    rest_l
        .len()
        .cmp(&rest_r.len())
        .then_with(|| left.cmp(right))
}

/// The run of digits at the start of `s`.
fn digits(s: &[u8]) -> &[u8] {
    &s[..s.iter().take_while(|b| b.is_ascii_digit()).count()]
}

fn trim_zeros(s: &[u8]) -> &[u8] {
    &s[s.iter().take_while(|&&b| b == b'0').count()..]
}
