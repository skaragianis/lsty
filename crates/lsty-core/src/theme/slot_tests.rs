use super::Slot;
use crate::todotxt::Priority;

#[test]
fn each_priority_letter_has_its_slot() {
    let slots = |letters: &[u8]| -> Vec<Slot> {
        letters
            .iter()
            .filter_map(|&l| Priority::new(l))
            .map(Slot::of_priority)
            .collect()
    };
    assert_eq!(
        slots(b"ABCDZ"),
        [
            Slot::PriorityA,
            Slot::PriorityB,
            Slot::PriorityC,
            Slot::PriorityOther,
            Slot::PriorityOther,
        ]
    );
}
