//! Allocation-free compiler diagnostic encoding of the existing wire descriptors.

#[cfg(test)]
#[path = "diagnostic/tests.rs"]
mod tests;
#[path = "diagnostic/writer.rs"]
mod writer;

use super::ConstantShape;
use writer::Writer;

/// Exact message length, or zero when the frozen shape is unchanged.
pub const fn encoded_len(
    stable_name: &str,
    version: u32,
    expected: &ConstantShape,
    actual: &ConstantShape,
) -> usize {
    if expected.same(actual) {
        return 0;
    }
    let mut writer = Writer::<0>::new(false);
    writer.observation(stable_name, version, expected, actual);
    writer.len()
}

/// Encode into the exact size returned by [`encoded_len`].
///
/// A mismatching buffer size panics. Equal shapes require an empty buffer and
/// skip descriptor serialization entirely.
pub const fn encode<const N: usize>(
    stable_name: &str,
    version: u32,
    expected: &ConstantShape,
    actual: &ConstantShape,
) -> [u8; N] {
    let mut writer = Writer::<N>::new(true);
    if !expected.same(actual) {
        writer.observation(stable_name, version, expected, actual);
    }
    writer.finish()
}
