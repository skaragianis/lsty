//! Any three lines merge without a panic into one line. No part clashes
//! twice, and no part clashes when two of the three lines are equal. A
//! draft equal to its base always merges and leaves the disk's line as it
//! is.
//! The merged line keeps the disk's done mark and dates. A clashing key
//! keeps the draft's value, and every project, context and tag the draft
//! added is in the merged line unless its key clashes or the disk's line
//! has white space at either end.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::ops::fuzzing::merge_property;

fuzz_target!(|data: &[u8]| merge_property(data));
