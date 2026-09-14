#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (u32, u32, String)| {
    type_history_fuzz::invoice_values(input.0, input.1, &input.2);
});
