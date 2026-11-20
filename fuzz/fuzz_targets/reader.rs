//! Scrolls, zooms and turns a view as the bytes say.

#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| pdvim::fuzz::reader(data));
