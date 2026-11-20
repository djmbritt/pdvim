//! Reads the bytes as a configuration file.

#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| pdvim::fuzz::config(data));
