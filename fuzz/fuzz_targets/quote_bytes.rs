//! Any bytes quote to text between double quotes with nothing unprintable
//! left raw, and reading the escapes back gives the same bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lsty_core::errs::fuzzing::quote_property;

fuzz_target!(|data: &[u8]| quote_property(data));
