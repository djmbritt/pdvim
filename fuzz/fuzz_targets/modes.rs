//! Types the bytes as keys at every mode that takes them.

#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| pdvim::fuzz::modes(data));
