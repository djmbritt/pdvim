//! Opens the bytes as an EPUB book, a comic, an image or another kind of
//! document besides PDF, picked by the first byte, and reads its first
//! pages.

#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| pdvim::fuzz::formats(data));
