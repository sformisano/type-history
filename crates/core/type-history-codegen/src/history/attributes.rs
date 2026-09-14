//! Closed field-history attribute grammar and assignment metadata.
use syn::{
    parse::{Parse, ParseStream},
    Attribute, Error, Expr, Ident, Path, Result, Token, Type,
};

/// How an added or updated field receives its historical value.
#[derive(Clone)]
pub enum Backfill {
    /// Ordinary expression checked against the destination field type.
    Value(Box<Expr>),
    /// Fallible callback borrowing the complete immediate predecessor payload.
    Function(Box<Path>),
}

/// One authored history record on one payload field.
#[derive(Clone)]
pub enum FieldHistoryRecord {
    /// The field is born at this version.
    Added {
        /// Birth version, strictly above V1.
        version: u32,
        /// Required explicit historical initialization.
        assignment: Backfill,
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
        assignment: Backfill,
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
/// Documentation and lint attributes are left untouched for generation.
pub fn parse_field_history(attributes: &[Attribute]) -> Result<FieldHistory> {
    let mut records = Vec::new();
    for attribute in attributes {
        if !attribute.path().is_ident("history") {
            continue;
        }
        records.push(attribute.parse_args::<FieldHistoryRecord>()?);
    }
    Ok(FieldHistory { records })
}

const CONFLICTING_RULES: &str =
    "a history record requires exactly one of `backfill_value` or `backfill_fn`";

impl Parse for FieldHistoryRecord {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let marker: Ident = input.parse()?;
        input.parse::<Token![=]>()?;

        let version = parse_version(input)?;
        let mut previous_type: Option<Type> = None;
        let mut backfill: Option<Expr> = None;
        let mut backfill_fn: Option<Path> = None;

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
                "backfill_fn" => {
                    assign_once(&mut backfill_fn, input.parse()?, &key, "backfill_fn")?;
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
            "added_in" => {
                if let Some(previous_type) = previous_type {
                    return Err(Error::new_spanned(
                        previous_type,
                        "`previous_type` describes an update boundary; an addition declares its birth type through the field declaration",
                    ));
                }
                let assignment = parse_backfill(backfill, backfill_fn, &marker)?;
                FieldHistoryRecord::Added {
                    version,
                    assignment,
                    marker,
                }
            }
            "updated_in" => {
                let Some(previous_type) = previous_type else {
                    return Err(Error::new_spanned(
                        &marker,
                        "an updated field requires `previous_type = PreviousType`",
                    ));
                };
                let assignment = parse_backfill(backfill, backfill_fn, &marker)?;
                FieldHistoryRecord::Updated {
                    version,
                    previous_type: Box::new(previous_type),
                    assignment,
                    marker,
                }
            }
            "removed_in" => {
                if previous_type.is_some() || backfill.is_some() || backfill_fn.is_some() {
                    return Err(Error::new_spanned(
                        &marker,
                        "a removal carries no backfill; the previous field stays available to backfill functions",
                    ));
                }
                FieldHistoryRecord::Removed { version, marker }
            }
            other => {
                return Err(Error::new_spanned(
                    &marker,
                    format!(
                        "unknown history record `{other}`; use `added_in`, `updated_in`, or `removed_in`"
                    ),
                ));
            }
        };
        Ok(record)
    }
}

fn parse_backfill(value: Option<Expr>, function: Option<Path>, marker: &Ident) -> Result<Backfill> {
    match (value, function) {
        (Some(value), None) => Ok(Backfill::Value(Box::new(value))),
        (None, Some(function)) => Ok(Backfill::Function(Box::new(function))),
        _ => Err(Error::new_spanned(marker, CONFLICTING_RULES)),
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
