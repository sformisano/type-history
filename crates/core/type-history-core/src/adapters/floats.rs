//! The dependency's Serde implementation requests the native width then checks finiteness.

use typed_floats::NonNaNFinite;

profile_traits!(NonNaNFinite<f32>, Finite32);
profile_traits!(NonNaNFinite<f64>, Finite64);

#[cfg(test)]
mod tests {
    use crate::resolved::{ResolvedSchema, SetMembership};
    use std::collections::{BTreeSet, HashSet};
    use typed_floats::NonNaNFinite;

    #[test]
    fn finite_zero_membership_ignores_sign_while_schema_keeps_width() {
        let positive = NonNaNFinite::<f32>::new(0.0).unwrap();
        let negative = NonNaNFinite::<f32>::new(-0.0).unwrap();
        assert_eq!(HashSet::from([positive, negative]).len(), 1);
        assert_eq!(BTreeSet::from([positive, negative]).len(), 1);
        assert_ne!(positive.get().to_bits(), negative.get().to_bits());
        assert_ne!(
            NonNaNFinite::<f32>::resolved_wire_schema(),
            NonNaNFinite::<f64>::resolved_wire_schema()
        );
        assert!(!NonNaNFinite::<f32>::MEMBERSHIP.same(&NonNaNFinite::<f64>::MEMBERSHIP));
    }
}
