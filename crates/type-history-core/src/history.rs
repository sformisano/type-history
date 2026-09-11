//! Static decoder descriptors and failure version context.

use serde::Serialize;

use crate::{DecodeError, PayloadVersion, StableName};

/// Exact-version decoder followed by adjacent typed conversions.
pub struct History<T, E = DecodeError> {
    retained_versions: &'static [PayloadVersion],
    decode: fn(PayloadVersion, &[u8]) -> Result<T, E>,
}

impl<T, E> Copy for History<T, E> {}

impl<T, E> Clone for History<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, E> History<T, E> {
    /// Construct the static contract emitted by a history macro.
    pub const fn new(
        retained_versions: &'static [PayloadVersion],
        decode: fn(PayloadVersion, &[u8]) -> Result<T, E>,
    ) -> Self {
        Self {
            retained_versions,
            decode,
        }
    }

    /// Every exact stored version the generated decoder accepts.
    pub const fn retained_versions(&self) -> &'static [PayloadVersion] {
        self.retained_versions
    }

    /// Whether the version belongs to this retained history.
    pub fn supports(&self, version: PayloadVersion) -> bool {
        self.retained_versions.contains(&version)
    }

    /// Decode original JSON bytes and convert to the current record.
    ///
    /// Generated decoders buffer the payload and require named fields for nested
    /// records. A type error found in buffered data may have no JSON position.
    pub fn decode(&self, version: PayloadVersion, bytes: &[u8]) -> Result<T, E> {
        (self.decode)(version, bytes)
    }
}

/// Stable classification of historical decoding failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DecodeFailureKind {
    /// Stored version is outside the exact retained inventory.
    UnsupportedVersion,
    /// Original bytes failed the exact historical record decoder.
    Decode,
    /// One explicitly authored adjacent conversion failed.
    Upcast,
}

/// Immutable original and adjacent version context for a decoding failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecodeContext {
    stable_name: StableName,
    source_version: PayloadVersion,
    adjacent_from: Option<PayloadVersion>,
    adjacent_to: Option<PayloadVersion>,
    kind: DecodeFailureKind,
}

impl DecodeContext {
    /// Construct context for an unsupported stored version.
    pub fn unsupported(stable_name: StableName, source_version: PayloadVersion) -> Self {
        Self {
            stable_name,
            source_version,
            adjacent_from: None,
            adjacent_to: None,
            kind: DecodeFailureKind::UnsupportedVersion,
        }
    }

    /// Construct context for failed historical JSON decoding.
    pub fn decode(stable_name: StableName, source_version: PayloadVersion) -> Self {
        Self {
            stable_name,
            source_version,
            adjacent_from: None,
            adjacent_to: None,
            kind: DecodeFailureKind::Decode,
        }
    }

    /// Construct context for one failing adjacent conversion.
    pub fn upcast(
        stable_name: StableName,
        source_version: PayloadVersion,
        adjacent_from: PayloadVersion,
        adjacent_to: PayloadVersion,
    ) -> Self {
        Self {
            stable_name,
            source_version,
            adjacent_from: Some(adjacent_from),
            adjacent_to: Some(adjacent_to),
            kind: DecodeFailureKind::Upcast,
        }
    }

    /// Stable name shared by every version of this type.
    pub fn stable_name(&self) -> &StableName {
        &self.stable_name
    }

    /// Original stored version, preserved throughout the conversion chain.
    pub const fn source_version(&self) -> PayloadVersion {
        self.source_version
    }

    /// Failing adjacent predecessor, if conversion started.
    pub const fn adjacent_from(&self) -> Option<PayloadVersion> {
        self.adjacent_from
    }

    /// Failing adjacent destination, if conversion started.
    pub const fn adjacent_to(&self) -> Option<PayloadVersion> {
        self.adjacent_to
    }

    /// Stable decode failure class.
    pub const fn kind(&self) -> DecodeFailureKind {
        self.kind
    }
}

#[cfg(test)]
mod tests {
    use super::History;
    use crate::PayloadVersion;

    #[test]
    fn descriptors_are_copy_without_payload_or_error_clone_bounds() {
        struct Payload;
        struct Failure;

        fn decode(_: PayloadVersion, _: &[u8]) -> Result<Payload, Failure> {
            Ok(Payload)
        }

        let descriptor = History::new(&[PayloadVersion::INITIAL], decode);
        let copied = descriptor;
        assert!(descriptor.supports(PayloadVersion::INITIAL));
        assert_eq!(copied.retained_versions(), &[PayloadVersion::INITIAL]);
        assert!(copied.decode(PayloadVersion::INITIAL, b"{}").is_ok());
    }
}
