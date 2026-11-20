//! Opens the bytes as a PDF and reads its first pages.

#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| pdvim::fuzz::pdf(data));
