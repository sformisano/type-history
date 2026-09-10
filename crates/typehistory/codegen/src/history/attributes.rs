//! Closed field-history attribute grammar and assignment metadata.
use syn::{
    parse::{Parse, ParseStream},
    Attribute, Error, Expr, Ident, Path, Result, Token, Type,
};

/// How an added field receives its value at its birth version.
#[derive(Clone)]
pub enum BirthAssignment {
    /// Ordinary expression checked against the birth type.
    BackfillValue(Box<Expr>),
    /// Fallible callback borrowing the immediate predecessor payload.
    CastFromPreviousPayload(Box<Path>),
}

/// How an updated field is produced from its predecessor.
#[derive(Clone)]
pub enum UpdateAssignment {
    /// Consumes the exact predecessor field.
    CastFromPreviousValue(Box<Path>),
    /// Borrows the whole immediate predecessor payload.
    CastFromPreviousPayload(Box<Path>),
}

/// One authored history record on one payload field.
#[derive(Clone)]
pub enum FieldHistoryRecord {
    /// The field is born at this version.
    Added {
        /// Birth version, strictly above the retained floor.
        version: u32,
        /// Required explicit historical initialization.
        assignment: BirthAssignment,
        /// Span carrier for diagnostics.
        marker: Ident,
    },
    /// The field changes type or meaning at this version.
    Updated {
        /// Boundary version.
        version: u32,
        /// Exact predecessor field type.
        previous_type: Box<Type>,
        /// Required explicit conversion.
        assignment: UpdateAssignment,
        /// Span carrier for diagnostics.
        marker: Ident,
    },
    /// The field is absent from this version onward.
    Removed {
        /// Terminal boundary version.
        version: u32,
        /// Span carrier for diagnostics.
        marker: Ident,
    },
}

impl FieldHistoryRecord {
    /// Boundary version this record describes.
    pub fn version(&self) -> u32 {
        match self {
            Self::Added { version, .. }
            | Self::Updated { version, .. }
            | Self::Removed { version, .. } => *version,
        }
    }

    pub(super) fn marker(&self) -> &Ident {
        match self {
            Self::Added { marker, .. }
            | Self::Updated { marker, .. }
            | Self::Removed { marker, .. } => marker,
        }
    }

    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::Added { .. } => "added_at",
            Self::Updated { .. } => "updated_at",
            Self::Removed { .. } => "removed_at",
        }
    }
}

/// Parsed `#[history(...)]` records for one payload field, newest first.
#[derive(Clone, Default)]
pub struct FieldHistory {
    /// Authored records in their declared newest-first order.
    pub records: Vec<FieldHistoryRecord>,
}

impl FieldHistory {
    /// Highest boundary version this field contributes to version inference.
    pub fn highest_boundary(&self) -> Option<u32> {
        self.records.iter().map(FieldHistoryRecord::version).max()
    }
}

/// Parse every `#[history(...)]` attribute on a payload field.
///
/// Non-history attributes are left untouched for the existing redaction and
/// lint handling.
pub fn parse_field_history(attributes: &[Attribute]) -> Result<FieldHistory> {
    let mut records = Vec::new();
    for attribute in attributes {
        if !attribute.path().is_ident("history") {
            continue;
        }
        let record: ParsedRecord = attribute.parse_args()?;
        match record {
            ParsedRecord::Field(record) => records.push(record),
            ParsedRecord::RetainedFrom { marker, .. } => {
                return Err(Error::new_spanned(
                    marker,
                    "`retained_from` declares an imported event floor; place it on the event, not on a field",
                ));
            }
        }
    }
    Ok(FieldHistory { records })
}

/// Parse an optional record-level `#[history(retained_from = vN)]` attribute.
pub fn parse_retained_floor(attributes: &[Attribute]) -> Result<Option<u32>> {
    let mut floor = None;
    for attribute in attributes {
        if !attribute.path().is_ident("history") {
            continue;
        }
        let record: ParsedRecord = attribute.parse_args()?;
        match record {
            ParsedRecord::RetainedFrom { version, marker } => {
                if floor.is_some() {
                    return Err(Error::new_spanned(
                        marker,
                        "an event declares at most one retained floor",
                    ));
                }
                floor = Some(version);
            }
            ParsedRecord::Field(record) => {
                return Err(Error::new_spanned(
                    record.marker(),
                    format!(
                        "`{}` describes one payload field; place it on the field, not on the event",
                        record.label()
                    ),
                ));
            }
        }
    }
    Ok(floor)
}

enum ParsedRecord {
    Field(FieldHistoryRecord),
    RetainedFrom { version: u32, marker: Ident },
}

const CONFLICTING_RULES: &str =
    "a history record carries exactly one assignment rule: `backfill_value`, `cast_from_previous_value`, or `cast_from_previous_payload`";

impl Parse for ParsedRecord {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let marker: Ident = input.parse()?;
        input.parse::<Token![=]>()?;

        if marker == "retained_from" {
            let version = parse_version(input)?;
            reject_trailing(input, &marker)?;
            return Ok(Self::RetainedFrom { version, marker });
        }

        let version = parse_version(input)?;
        let mut previous_type: Option<Type> = None;
        let mut backfill: Option<Expr> = None;
        let mut previous_value: Option<Path> = None;
        let mut previous_payload: Option<Path> = None;

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            match key.to_string().as_str() {
                "previous_type" => {
                    assign_once(&mut previous_type, input.parse()?, &key, "previous_type")?
                }
                "backfill_value" => {
                    assign_once(&mut backfill, input.parse()?, &key, "backfill_value")?;
                }
                "cast_from_previous_value" => {
                    assign_once(
                        &mut previous_value,
                        input.parse()?,
                        &key,
                        "cast_from_previous_value",
                    )?;
                }
                "cast_from_previous_payload" => {
                    assign_once(
                        &mut previous_payload,
                        input.parse()?,
                        &key,
                        "cast_from_previous_payload",
                    )?;
                }
                other => {
                    return Err(Error::new_spanned(
                        &key,
                        format!("unknown history key `{other}`"),
                    ));
                }
            }
        }
        reject_trailing(input, &marker)?;

        let record = match marker.to_string().as_str() {
            "added_at" => {
                if let Some(previous_type) = previous_type {
                    return Err(Error::new_spanned(
                        previous_type,
                        "`previous_type` describes an update boundary; an addition declares its birth type through the field declaration",
                    ));
                }
                if previous_value.is_some() {
                    return Err(Error::new_spanned(
                        &marker,
                        "an added field has no predecessor value; use `backfill_value` or `cast_from_previous_payload`",
                    ));
                }
                let assignment = match (backfill, previous_payload) {
                    (Some(expression), None) => {
                        BirthAssignment::BackfillValue(Box::new(expression))
                    }
                    (None, Some(path)) => BirthAssignment::CastFromPreviousPayload(Box::new(path)),
                    (None, None) => {
                        return Err(Error::new_spanned(
                            &marker,
                            "an added field requires `backfill_value` or `cast_from_previous_payload`",
                        ));
                    }
                    (Some(_), Some(_)) => {
                        return Err(Error::new_spanned(&marker, CONFLICTING_RULES));
                    }
                };
                FieldHistoryRecord::Added {
                    version,
                    assignment,
                    marker,
                }
            }
            "updated_at" => {
                let Some(previous_type) = previous_type else {
                    return Err(Error::new_spanned(
                        &marker,
                        "an updated field requires `previous_type = PreviousType`",
                    ));
                };
                if backfill.is_some() {
                    return Err(Error::new_spanned(
                        &marker,
                        "`backfill_value` initializes an added field; an update converts its predecessor",
                    ));
                }
                let assignment = match (previous_value, previous_payload) {
                    (Some(path), None) => UpdateAssignment::CastFromPreviousValue(Box::new(path)),
                    (None, Some(path)) => UpdateAssignment::CastFromPreviousPayload(Box::new(path)),
                    (None, None) => {
                        return Err(Error::new_spanned(
                            &marker,
                            "an updated field requires `cast_from_previous_value` or `cast_from_previous_payload`",
                        ));
                    }
                    (Some(_), Some(_)) => {
                        return Err(Error::new_spanned(&marker, CONFLICTING_RULES));
                    }
                };
                FieldHistoryRecord::Updated {
                    version,
                    previous_type: Box::new(previous_type),
                    assignment,
                    marker,
                }
            }
            "removed_at" => {
                if previous_type.is_some()
                    || backfill.is_some()
                    || previous_value.is_some()
                    || previous_payload.is_some()
                {
                    return Err(Error::new_spanned(
                        &marker,
                        "a removal carries no conversion; the previous value stays available to payload callbacks",
                    ));
                }
                FieldHistoryRecord::Removed { version, marker }
            }
            other => {
                return Err(Error::new_spanned(
                    &marker,
                    format!(
                        "unknown history record `{other}`; use `added_at`, `updated_at`, `removed_at`, or `retained_from`"
                    ),
                ));
            }
        };
        Ok(Self::Field(record))
    }
}

fn assign_once<T>(slot: &mut Option<T>, value: T, key: &Ident, name: &str) -> Result<()> {
    if slot.is_some() {
        return Err(Error::new_spanned(
            key,
            format!("duplicate history key `{name}`"),
        ));
    }
    *slot = Some(value);
    Ok(())
}

fn reject_trailing(input: ParseStream<'_>, marker: &Ident) -> Result<()> {
    if input.is_empty() {
        return Ok(());
    }
    Err(Error::new_spanned(
        marker,
        "unexpected tokens after the history record",
    ))
}

/// Parse a `vN` version token into its positive numeric value.
fn parse_version(input: ParseStream<'_>) -> Result<u32> {
    let token: Ident = input.parse()?;
    let text = token.to_string();
    let digits = text.strip_prefix('v').ok_or_else(|| {
        Error::new_spanned(&token, "a history version is written as `v1`, `v2`, `v3`")
    })?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::new_spanned(
            &token,
            "a history version is written as `v1`, `v2`, `v3`",
        ));
    }
    let value: u32 = digits.parse().map_err(|_| {
        Error::new_spanned(
            &token,
            "history version exceeds the supported positive range",
        )
    })?;
    if value == 0 {
        return Err(Error::new_spanned(
            &token,
            "history versions are positive; `v0` is not a version",
        ));
    }
    Ok(value)
}
