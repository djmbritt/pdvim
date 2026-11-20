//! Parses and completes the bytes as a line typed after `:`.

#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| pdvim::fuzz::command(data));
