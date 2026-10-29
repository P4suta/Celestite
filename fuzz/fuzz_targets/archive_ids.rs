#![no_main]
use libfuzzer_sys::fuzz_target;
#[path = "../invariants.rs"]
mod invariants;

fuzz_target!(|data: &[u8]| {
    let result = invariants::check(data);
    assert!(result.is_ok(), "{result:?}");
});
