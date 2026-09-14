use super::super::{
    ConstantFields, ConstantItems, ConstantName, ConstantShape, ConstantVariant, ConstantVariants,
};

/// Counting and writing share the same traversal, including escaping rules.
pub(super) struct Writer<const N: usize> {
    bytes: [u8; N],
    length: usize,
    write: bool,
}

impl<const N: usize> Writer<N> {
    pub(super) const fn new(write: bool) -> Self {
        Self {
            bytes: [0; N],
            length: 0,
            write,
        }
    }
    pub(super) const fn len(&self) -> usize {
        self.length
    }
    pub(super) const fn finish(self) -> [u8; N] {
        assert!(
            self.length == N,
            "incorrect frozen schema diagnostic buffer size"
        );
        self.bytes
    }

    pub(super) const fn observation(
        &mut self,
        name: &str,
        version: u32,
        expected: &ConstantShape,
        actual: &ConstantShape,
    ) {
        self.text("frozen payload changed; restore its history and add the next version. TYPE_HISTORY_SCHEMA_DIAGNOSTIC_V1:{\"stable_name\":");
        self.string(name);
        self.text(",\"version\":");
        self.number(version as usize);
        self.text(",\"expected\":");
        self.shape(expected);
        self.text(",\"actual\":");
        self.shape(actual);
        self.byte(b'}');
    }

    const fn byte(&mut self, byte: u8) {
        if self.write {
            self.bytes[self.length] = byte;
        }
        self.length += 1;
    }
    const fn text(&mut self, value: &str) {
        let bytes = value.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            self.byte(bytes[index]);
            index += 1;
        }
    }
    const fn escaped(&mut self, byte: u8) {
        match byte {
            b'"' => self.text("\\\""),
            b'\\' => self.text("\\\\"),
            b'\n' => self.text("\\n"),
            b'\r' => self.text("\\r"),
            b'\t' => self.text("\\t"),
            8 => self.text("\\b"),
            12 => self.text("\\f"),
            0..=31 => {
                self.text("\\u00");
                self.byte(b"0123456789abcdef"[(byte >> 4) as usize]);
                self.byte(b"0123456789abcdef"[(byte & 15) as usize]);
            }
            _ => self.byte(byte),
        }
    }
    const fn string(&mut self, value: &str) {
        self.byte(b'"');
        let bytes = value.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            self.escaped(bytes[index]);
            index += 1;
        }
        self.byte(b'"');
    }
    const fn name(&mut self, mut name: &ConstantName) {
        self.byte(b'"');
        while let ConstantName::Byte(byte, tail) = name {
            self.escaped(*byte);
            name = tail;
        }
        self.byte(b'"');
    }
    const fn number(&mut self, mut value: usize) {
        let mut divisor = 1;
        while value / divisor >= 10 {
            divisor *= 10;
        }
        loop {
            self.byte(b'0' + (value / divisor) as u8);
            value %= divisor;
            if divisor == 1 {
                break;
            }
            divisor /= 10;
        }
    }
    const fn shape(&mut self, shape: &ConstantShape) {
        self.text("{\"kind\":");
        match shape {
            ConstantShape::Bool => self.string("bool"),
            ConstantShape::I8 => self.string("i8"),
            ConstantShape::I16 => self.string("i16"),
            ConstantShape::I32 => self.string("i32"),
            ConstantShape::I64 => self.string("i64"),
            ConstantShape::I128 => self.string("i128"),
            ConstantShape::U8 => self.string("u8"),
            ConstantShape::U16 => self.string("u16"),
            ConstantShape::U32 => self.string("u32"),
            ConstantShape::U64 => self.string("u64"),
            ConstantShape::U128 => self.string("u128"),
            ConstantShape::String => self.string("string"),
            ConstantShape::Bytes => self.string("bytes"),
            ConstantShape::Option(value) => {
                self.string("option");
                self.value(value);
            }
            ConstantShape::Sequence(value) => {
                self.string("sequence");
                self.value(value);
            }
            ConstantShape::Array(value, length) => {
                self.string("array");
                self.value(value);
                self.text(",\"length\":");
                self.number(*length);
            }
            ConstantShape::Record(fields) => {
                self.string("record");
                self.fields(fields);
            }
            ConstantShape::Enum(variants) => {
                self.string("enum");
                self.text(",\"variants\":[");
                let mut next = variants;
                let mut first = true;
                while let ConstantVariants::Variant(name, variant, tail) = next {
                    if !first {
                        self.byte(b',');
                    }
                    first = false;
                    self.text("{\"name\":");
                    self.name(name);
                    self.text(",\"kind\":");
                    self.variant(variant);
                    self.byte(b'}');
                    next = tail;
                }
                self.byte(b']');
            }
        }
        self.byte(b'}');
    }
    const fn value(&mut self, value: &ConstantShape) {
        self.text(",\"value\":");
        self.shape(value);
    }
    const fn fields(&mut self, mut next: &ConstantFields) {
        self.text(",\"fields\":[");
        let mut first = true;
        while let ConstantFields::Field(name, shape, tail) = next {
            if !first {
                self.byte(b',');
            }
            first = false;
            self.text("{\"name\":");
            self.name(name);
            self.text(",\"presence\":");
            if matches!(shape, ConstantShape::Option(_)) {
                self.string("optional");
            } else {
                self.string("required");
            }
            self.text(",\"schema\":");
            self.shape(shape);
            self.byte(b'}');
            next = tail;
        }
        self.byte(b']');
    }
    const fn variant(&mut self, variant: &ConstantVariant) {
        match variant {
            ConstantVariant::Unit => self.string("unit"),
            ConstantVariant::Newtype(ConstantShape::Record(fields))
            | ConstantVariant::Record(fields) => {
                self.string("record");
                self.fields(fields);
            }
            ConstantVariant::Newtype(value) => {
                self.string("newtype");
                self.text(",\"schema\":");
                self.shape(value);
            }
            ConstantVariant::Tuple(items) => {
                self.string("tuple");
                self.text(",\"items\":[");
                let mut next = items;
                let mut first = true;
                while let ConstantItems::Item(shape, tail) = next {
                    if !first {
                        self.byte(b',');
                    }
                    first = false;
                    self.shape(shape);
                    next = tail;
                }
                self.byte(b']');
            }
        }
    }
}
