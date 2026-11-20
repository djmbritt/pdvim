//! Parses the bytes as keys written in the configuration, and back.

#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| pdvim::fuzz::keys(data));
