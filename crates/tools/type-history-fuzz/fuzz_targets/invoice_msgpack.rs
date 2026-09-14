#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| type_history_fuzz::invoice_msgpack(bytes));
