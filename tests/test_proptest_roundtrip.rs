//! Property-based roundtrip tests between serialization formats.
//!
//! Uses proptest to generate arbitrary `Model` instances and verify that
//! serializing to one format and deserializing back produces an equivalent model.
//! Tests cover all pairwise combinations of XMI, YAML, and JSON-LD.
//!
//! ## Known format differences
//!
//! - **XMI** stores all XML attribute values as untyped strings, so custom
//!   properties lose their type (e.g. `Integer(42)` -> `String("42")`).
//!   Tests involving XMI use type-coercing comparison.
//!
//! - **XMI** represents relationships as relationship-kind child elements
//!   within the ownership tree, not as standalone objects in `model.relationships`.
//!   The XMI writer does not serialize standalone `Relationship` entries,
//!   so relationship roundtrip tests are restricted to YAML and JSON-LD.
//!
//! - **YAML** and **JSON-LD** preserve property types natively and serialize
//!   standalone relationships, so their pairwise tests are fully strict.
#![cfg(all(feature = "interchange", feature = "proptest"))]

use indexmap::IndexMap;
use proptest::prelude::*;
use std::sync::Arc;
use syster::interchange::model::*;
use syster::interchange::{JsonLd, ModelFormat, Xmi, Yaml};

// ============================================================================
// PROPTEST STRATEGIES
// ============================================================================

/// Strategy for element names (valid SysML identifiers).
fn arb_name() -> impl Strategy<Value = Arc<str>> {
    "[A-Z][a-zA-Z0-9_]{0,20}".prop_map(|s| Arc::from(s.as_str()))
}

/// Strategy for optional element names.
fn arb_opt_name() -> impl Strategy<Value = Option<Arc<str>>> {
    prop_oneof![Just(None), arb_name().prop_map(Some)]
}

/// Strategy for optional short names.
fn arb_opt_short_name() -> impl Strategy<Value = Option<Arc<str>>> {
    prop_oneof![
        3 => Just(None),
        1 => "[a-z]{1,5}".prop_map(|s| Some(Arc::from(s.as_str()))),
    ]
}

/// Strategy for element kinds suitable for interchange testing.
/// Excludes relationship-typed ElementKinds (those are serialized as
/// child elements in XMI, not as standalone elements) and internal
/// expression/literal kinds that have special serialization rules.
fn arb_element_kind() -> impl Strategy<Value = ElementKind> {
    prop_oneof![
        Just(ElementKind::Package),
        Just(ElementKind::LibraryPackage),
        Just(ElementKind::Namespace),
        Just(ElementKind::Class),
        Just(ElementKind::DataType),
        Just(ElementKind::Structure),
        Just(ElementKind::Association),
        Just(ElementKind::Behavior),
        Just(ElementKind::Function),
        Just(ElementKind::Predicate),
        Just(ElementKind::PartDefinition),
        Just(ElementKind::ItemDefinition),
        Just(ElementKind::ActionDefinition),
        Just(ElementKind::PortDefinition),
        Just(ElementKind::AttributeDefinition),
        Just(ElementKind::ConnectionDefinition),
        Just(ElementKind::InterfaceDefinition),
        Just(ElementKind::AllocationDefinition),
        Just(ElementKind::RequirementDefinition),
        Just(ElementKind::ConstraintDefinition),
        Just(ElementKind::StateDefinition),
        Just(ElementKind::CalculationDefinition),
        Just(ElementKind::EnumerationDefinition),
        Just(ElementKind::PartUsage),
        Just(ElementKind::ItemUsage),
        Just(ElementKind::ActionUsage),
        Just(ElementKind::PortUsage),
        Just(ElementKind::AttributeUsage),
        Just(ElementKind::ConnectionUsage),
        Just(ElementKind::ReferenceUsage),
        Just(ElementKind::StateUsage),
        Just(ElementKind::ConstraintUsage),
        Just(ElementKind::Feature),
    ]
}

/// Strategy for property values that survive all serialization formats.
fn arb_property_value() -> impl Strategy<Value = PropertyValue> {
    prop_oneof![
        "[a-zA-Z0-9_ ]{1,30}".prop_map(|s| PropertyValue::String(Arc::from(s.as_str()))),
        (-1000i64..1000i64).prop_map(PropertyValue::Integer),
        (-100.0f64..100.0f64)
            .prop_map(|f| PropertyValue::Real((f * 100.0).round() / 100.0)),
        any::<bool>().prop_map(PropertyValue::Boolean),
    ]
}

/// Boolean flag property keys handled as top-level fields by serializers.
const BOOLEAN_FLAG_KEYS: &[&str] = &[
    "isAbstract",
    "isVariation",
    "isDerived",
    "isReadOnly",
    "isOrdered",
    "isNonunique",
    "isParallel",
    "isIndividual",
    "isEnd",
    "isDefault",
    "isPortion",
];

/// Strategy for a map of custom properties with safe key names.
fn arb_properties() -> impl Strategy<Value = IndexMap<Arc<str>, PropertyValue>> {
    proptest::collection::vec(
        (
            prop_oneof![
                Just("customProp1"),
                Just("customProp2"),
                Just("tag"),
                Just("priority"),
                Just("notes"),
            ],
            arb_property_value(),
        ),
        0..=3,
    )
    .prop_map(|pairs| {
        pairs
            .into_iter()
            .map(|(k, v)| (Arc::from(k), v))
            .collect()
    })
}

/// Strategy for relationship kinds.
fn arb_relationship_kind() -> impl Strategy<Value = RelationshipKind> {
    prop_oneof![
        Just(RelationshipKind::Specialization),
        Just(RelationshipKind::FeatureTyping),
        Just(RelationshipKind::Subsetting),
        Just(RelationshipKind::Redefinition),
        Just(RelationshipKind::Membership),
        Just(RelationshipKind::OwningMembership),
        Just(RelationshipKind::FeatureMembership),
        Just(RelationshipKind::NamespaceImport),
        Just(RelationshipKind::MembershipImport),
        Just(RelationshipKind::FeatureChaining),
        Just(RelationshipKind::Disjoining),
    ]
}

/// Strategy for a model with elements only (no standalone relationships).
/// Suitable for XMI roundtrip testing since XMI doesn't serialize
/// standalone `model.relationships` entries.
fn arb_model_elements_only() -> impl Strategy<Value = Model> {
    (1usize..=5usize).prop_flat_map(|n_elems| {
        proptest::collection::vec(
            (
                arb_element_kind(),
                arb_opt_name(),
                arb_opt_short_name(),
                any::<bool>(),
                any::<bool>(),
                any::<bool>(),
                arb_properties(),
            ),
            n_elems..=n_elems,
        )
        .prop_map(|elem_data| {
            build_model(elem_data, Vec::new())
        })
    })
}

/// Strategy for a model with elements and standalone relationships.
/// Suitable for YAML and JSON-LD roundtrip testing.
fn arb_model_with_relationships() -> impl Strategy<Value = Model> {
    (1usize..=5usize).prop_flat_map(|n_elems| {
        let elem_data = proptest::collection::vec(
            (
                arb_element_kind(),
                arb_opt_name(),
                arb_opt_short_name(),
                any::<bool>(),
                any::<bool>(),
                any::<bool>(),
                arb_properties(),
            ),
            n_elems..=n_elems,
        );

        let max_rels = std::cmp::min(n_elems.saturating_sub(1), 4);
        let rel_data = proptest::collection::vec(
            (arb_relationship_kind(), 0..n_elems, 0..n_elems),
            0..=max_rels,
        );

        (elem_data, rel_data)
    })
    .prop_map(|(elem_data, rel_data)| {
        build_model(elem_data, rel_data)
    })
}

/// Build a Model from generated element and relationship data.
fn build_model(
    elem_data: Vec<(
        ElementKind,
        Option<Arc<str>>,
        Option<Arc<str>>,
        bool,
        bool,
        bool,
        IndexMap<Arc<str>, PropertyValue>,
    )>,
    rel_data: Vec<(RelationshipKind, usize, usize)>,
) -> Model {
    let mut model = Model::new();
    let mut elem_ids: Vec<ElementId> = Vec::new();
    let n_elems = elem_data.len();

    for (i, (kind, name, short_name, is_abstract, is_derived, is_readonly, properties)) in
        elem_data.into_iter().enumerate()
    {
        let eid = ElementId::new(format!("elem-{i}"));
        elem_ids.push(eid.clone());

        let mut elem = Element::new(eid, kind);
        elem.name = name;
        elem.short_name = short_name;

        if is_abstract {
            elem.set_abstract(true);
        }
        if is_derived {
            elem.set_derived(true);
        }
        if is_readonly {
            elem.set_readonly(true);
        }

        for (key, value) in properties {
            if !BOOLEAN_FLAG_KEYS.contains(&key.as_ref()) {
                elem.properties.insert(key, value);
            }
        }

        // First element is root, rest are children
        if i > 0 {
            elem.owner = Some(elem_ids[0].clone());
        }

        model.add_element(elem);
    }

    if n_elems > 1 {
        let root_id = elem_ids[0].clone();
        if let Some(root) = model.elements.get_mut(&root_id) {
            for eid in elem_ids.iter().skip(1) {
                root.owned_elements.push(eid.clone());
            }
        }
    }

    for (i, (kind, src_idx, tgt_idx)) in rel_data.into_iter().enumerate() {
        let source = elem_ids[src_idx].clone();
        let target = elem_ids[tgt_idx].clone();
        let mut rel = Relationship::new(format!("rel-{i}"), kind, source.clone(), target);
        rel.owner = Some(source);
        model.add_relationship(rel);
    }

    model
}

// ============================================================================
// MODEL COMPARISON
// ============================================================================

/// Whether to use type-coercing comparison for property values.
#[derive(Clone, Copy)]
enum CompareMode {
    /// Strict type equality (YAML <-> JSON-LD).
    Strict,
    /// Type-coercing comparison (anything involving XMI).
    Coercing,
}

/// Whether to compare relationships.
#[derive(Clone, Copy)]
enum RelCompare {
    /// Compare relationships (YAML/JSON-LD roundtrips).
    Yes,
    /// Skip relationship comparison (XMI roundtrips).
    Skip,
}

/// Compare two models for semantic equivalence.
fn models_equivalent(
    original: &Model,
    roundtripped: &Model,
    mode: CompareMode,
    rel_compare: RelCompare,
) -> Result<(), String> {
    // Element count
    if original.element_count() != roundtripped.element_count() {
        return Err(format!(
            "Element count mismatch: {} vs {}",
            original.element_count(),
            roundtripped.element_count()
        ));
    }

    // Compare each element by ID
    for (id, orig) in &original.elements {
        let rt = roundtripped
            .elements
            .get(id)
            .ok_or_else(|| format!("Missing element: {}", id))?;

        if orig.kind != rt.kind {
            return Err(format!(
                "Element {} kind mismatch: {:?} vs {:?}",
                id, orig.kind, rt.kind
            ));
        }
        if orig.name != rt.name {
            return Err(format!(
                "Element {} name mismatch: {:?} vs {:?}",
                id, orig.name, rt.name
            ));
        }
        if orig.short_name != rt.short_name {
            return Err(format!(
                "Element {} short_name mismatch: {:?} vs {:?}",
                id, orig.short_name, rt.short_name
            ));
        }

        // Boolean flags
        for (flag_name, orig_val, rt_val) in [
            ("is_abstract", orig.is_abstract, rt.is_abstract),
            ("is_derived", orig.is_derived, rt.is_derived),
            ("is_readonly", orig.is_readonly, rt.is_readonly),
            ("is_variation", orig.is_variation, rt.is_variation),
            ("is_ordered", orig.is_ordered, rt.is_ordered),
            ("is_nonunique", orig.is_nonunique, rt.is_nonunique),
        ] {
            if orig_val != rt_val {
                return Err(format!(
                    "Element {} {}: {} vs {}",
                    id, flag_name, orig_val, rt_val
                ));
            }
        }

        // Custom properties (excluding boolean flags)
        let orig_custom: IndexMap<_, _> = orig
            .properties
            .iter()
            .filter(|(k, _)| !BOOLEAN_FLAG_KEYS.contains(&k.as_ref()))
            .collect();
        let rt_custom: IndexMap<_, _> = rt
            .properties
            .iter()
            .filter(|(k, _)| !BOOLEAN_FLAG_KEYS.contains(&k.as_ref()))
            .collect();

        for (key, orig_val) in &orig_custom {
            let rt_val = rt_custom.get(key).ok_or_else(|| {
                format!(
                    "Element {} missing property '{}' (value: {:?})",
                    id, key, orig_val
                )
            })?;

            let equal = match mode {
                CompareMode::Strict => property_values_equal_strict(orig_val, rt_val),
                CompareMode::Coercing => property_values_equal_coercing(orig_val, rt_val),
            };
            if !equal {
                return Err(format!(
                    "Element {} property '{}' mismatch: {:?} vs {:?}",
                    id, key, orig_val, rt_val
                ));
            }
        }
    }

    // Relationships
    if let RelCompare::Yes = rel_compare {
        if original.relationship_count() != roundtripped.relationship_count() {
            return Err(format!(
                "Relationship count mismatch: {} vs {}",
                original.relationship_count(),
                roundtripped.relationship_count()
            ));
        }

        let orig_rels: std::collections::HashMap<_, _> = original
            .relationships
            .iter()
            .map(|r| (r.id.as_str().to_string(), r))
            .collect();
        let rt_rels: std::collections::HashMap<_, _> = roundtripped
            .relationships
            .iter()
            .map(|r| (r.id.as_str().to_string(), r))
            .collect();

        for (id, orig) in &orig_rels {
            let rt = rt_rels
                .get(id)
                .ok_or_else(|| format!("Missing relationship: {}", id))?;

            if orig.kind != rt.kind {
                return Err(format!(
                    "Relationship {} kind mismatch: {:?} vs {:?}",
                    id, orig.kind, rt.kind
                ));
            }
            if orig.source != rt.source {
                return Err(format!(
                    "Relationship {} source mismatch: {} vs {}",
                    id, orig.source, rt.source
                ));
            }
            if orig.target != rt.target {
                return Err(format!(
                    "Relationship {} target mismatch: {} vs {}",
                    id, orig.target, rt.target
                ));
            }
        }
    }

    Ok(())
}

/// Strict property value comparison (with float tolerance).
fn property_values_equal_strict(a: &PropertyValue, b: &PropertyValue) -> bool {
    match (a, b) {
        (PropertyValue::String(a), PropertyValue::String(b)) => a == b,
        (PropertyValue::Integer(a), PropertyValue::Integer(b)) => a == b,
        (PropertyValue::Boolean(a), PropertyValue::Boolean(b)) => a == b,
        (PropertyValue::Real(a), PropertyValue::Real(b)) => (a - b).abs() < 1e-6,
        (PropertyValue::Reference(a), PropertyValue::Reference(b)) => a == b,
        (PropertyValue::List(a), PropertyValue::List(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(x, y)| property_values_equal_strict(x, y))
        }
        _ => false,
    }
}

/// Type-coercing comparison for XMI roundtrips.
/// XMI stores all attributes as strings, so we fall back to string comparison.
fn property_values_equal_coercing(a: &PropertyValue, b: &PropertyValue) -> bool {
    if property_values_equal_strict(a, b) {
        return true;
    }
    property_value_to_string(a) == property_value_to_string(b)
}

/// Canonical string representation of a property value.
fn property_value_to_string(v: &PropertyValue) -> String {
    match v {
        PropertyValue::String(s) => s.to_string(),
        PropertyValue::Integer(i) => i.to_string(),
        PropertyValue::Real(f) => f.to_string(),
        PropertyValue::Boolean(b) => b.to_string(),
        PropertyValue::Reference(id) => id.as_str().to_string(),
        PropertyValue::List(items) => items
            .iter()
            .map(property_value_to_string)
            .collect::<Vec<_>>()
            .join(","),
    }
}

// ============================================================================
// ROUNDTRIP HELPERS
// ============================================================================

fn roundtrip_via(
    model: &Model,
    writer: &dyn ModelFormat,
    reader: &dyn ModelFormat,
    mode: CompareMode,
    rel: RelCompare,
) -> Result<(), String> {
    let bytes = writer
        .write(model)
        .map_err(|e| format!("{} write error: {}", writer.name(), e))?;
    let roundtripped = reader
        .read(&bytes)
        .map_err(|e| format!("{} read error: {}", reader.name(), e))?;
    models_equivalent(model, &roundtripped, mode, rel)
}

fn roundtrip_two_hop(
    model: &Model,
    format: &dyn ModelFormat,
    mode: CompareMode,
    rel: RelCompare,
) -> Result<(), String> {
    let bytes = format
        .write(model)
        .map_err(|e| format!("{} write error: {}", format.name(), e))?;
    let intermediate = format
        .read(&bytes)
        .map_err(|e| format!("{} read error: {}", format.name(), e))?;

    let bytes2 = format
        .write(&intermediate)
        .map_err(|e| format!("{} second write error: {}", format.name(), e))?;
    let final_model = format
        .read(&bytes2)
        .map_err(|e| format!("{} second read error: {}", format.name(), e))?;

    models_equivalent(&intermediate, &final_model, mode, rel)
}

fn roundtrip_chain(
    model: &Model,
    fmt1: &dyn ModelFormat,
    fmt2: &dyn ModelFormat,
    mode: CompareMode,
    rel: RelCompare,
) -> Result<(), String> {
    let bytes1 = fmt1
        .write(model)
        .map_err(|e| format!("{} write error: {}", fmt1.name(), e))?;
    let intermediate = fmt1
        .read(&bytes1)
        .map_err(|e| format!("{} read error: {}", fmt1.name(), e))?;

    let bytes2 = fmt2
        .write(&intermediate)
        .map_err(|e| format!("{} write error: {}", fmt2.name(), e))?;
    let final_model = fmt2
        .read(&bytes2)
        .map_err(|e| format!("{} read error: {}", fmt2.name(), e))?;

    models_equivalent(&intermediate, &final_model, mode, rel)
}

// ============================================================================
// SAME-FORMAT ROUNDTRIP TESTS
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn yaml_self_roundtrip(model in arb_model_with_relationships()) {
        roundtrip_via(&model, &Yaml, &Yaml, CompareMode::Strict, RelCompare::Yes)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }

    #[test]
    fn jsonld_self_roundtrip(model in arb_model_with_relationships()) {
        roundtrip_via(&model, &JsonLd, &JsonLd, CompareMode::Strict, RelCompare::Yes)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }

    #[test]
    fn xmi_self_roundtrip(model in arb_model_elements_only()) {
        roundtrip_via(&model, &Xmi, &Xmi, CompareMode::Coercing, RelCompare::Skip)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }
}

// ============================================================================
// TWO-HOP STABILITY TESTS
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn yaml_two_hop_stable(model in arb_model_with_relationships()) {
        roundtrip_two_hop(&model, &Yaml, CompareMode::Strict, RelCompare::Yes)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }

    #[test]
    fn jsonld_two_hop_stable(model in arb_model_with_relationships()) {
        roundtrip_two_hop(&model, &JsonLd, CompareMode::Strict, RelCompare::Yes)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }

    #[test]
    fn xmi_two_hop_stable(model in arb_model_elements_only()) {
        roundtrip_two_hop(&model, &Xmi, CompareMode::Coercing, RelCompare::Skip)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }
}

// ============================================================================
// CROSS-FORMAT CHAIN TESTS (YAML <-> JSON-LD: typed, strict)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn yaml_to_jsonld_chain(model in arb_model_with_relationships()) {
        roundtrip_chain(&model, &Yaml, &JsonLd, CompareMode::Strict, RelCompare::Yes)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }

    #[test]
    fn jsonld_to_yaml_chain(model in arb_model_with_relationships()) {
        roundtrip_chain(&model, &JsonLd, &Yaml, CompareMode::Strict, RelCompare::Yes)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }
}

// ============================================================================
// CROSS-FORMAT CHAIN TESTS (XMI involved: coercing, no standalone rels)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn xmi_to_yaml_chain(model in arb_model_elements_only()) {
        roundtrip_chain(&model, &Xmi, &Yaml, CompareMode::Coercing, RelCompare::Skip)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }

    #[test]
    fn xmi_to_jsonld_chain(model in arb_model_elements_only()) {
        roundtrip_chain(&model, &Xmi, &JsonLd, CompareMode::Coercing, RelCompare::Skip)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }

    #[test]
    fn yaml_to_xmi_chain(model in arb_model_elements_only()) {
        roundtrip_chain(&model, &Yaml, &Xmi, CompareMode::Coercing, RelCompare::Skip)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }

    #[test]
    fn jsonld_to_xmi_chain(model in arb_model_elements_only()) {
        roundtrip_chain(&model, &JsonLd, &Xmi, CompareMode::Coercing, RelCompare::Skip)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }
}

// ============================================================================
// THREE-FORMAT CHAIN TESTS
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// XMI -> YAML -> JSON-LD: after XMI normalizes types, YAML<->JSON-LD is strict.
    #[test]
    fn xmi_yaml_jsonld_chain(model in arb_model_elements_only()) {
        let xmi_bytes = Xmi.write(&model).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let from_xmi = Xmi.read(&xmi_bytes).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;

        let yaml_bytes = Yaml.write(&from_xmi).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let from_yaml = Yaml.read(&yaml_bytes).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;

        let json_bytes = JsonLd.write(&from_yaml).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let from_json = JsonLd.read(&json_bytes).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;

        models_equivalent(&from_yaml, &from_json, CompareMode::Strict, RelCompare::Skip)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }

    /// YAML -> JSON-LD -> XMI: last hop coerces types.
    #[test]
    fn yaml_jsonld_xmi_chain(model in arb_model_elements_only()) {
        let yaml_bytes = Yaml.write(&model).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let from_yaml = Yaml.read(&yaml_bytes).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;

        let json_bytes = JsonLd.write(&from_yaml).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let from_json = JsonLd.read(&json_bytes).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;

        let xmi_bytes = Xmi.write(&from_json).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let from_xmi = Xmi.read(&xmi_bytes).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;

        models_equivalent(&from_json, &from_xmi, CompareMode::Coercing, RelCompare::Skip)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }

    /// JSON-LD -> XMI -> YAML: XMI in the middle normalizes types.
    #[test]
    fn jsonld_xmi_yaml_chain(model in arb_model_elements_only()) {
        let json_bytes = JsonLd.write(&model).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let from_json = JsonLd.read(&json_bytes).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;

        let xmi_bytes = Xmi.write(&from_json).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let from_xmi = Xmi.read(&xmi_bytes).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;

        let yaml_bytes = Yaml.write(&from_xmi).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let from_yaml = Yaml.read(&yaml_bytes).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;

        models_equivalent(&from_xmi, &from_yaml, CompareMode::Strict, RelCompare::Skip)
            .map_err(|e| TestCaseError::Fail(e.into()))?;
    }
}

// ============================================================================
// STRUCTURAL PROPERTY TESTS
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Serialized output is never empty.
    #[test]
    fn serialization_produces_nonempty_output(model in arb_model_elements_only()) {
        let xmi_bytes = Xmi.write(&model).expect("XMI write failed");
        let yaml_bytes = Yaml.write(&model).expect("YAML write failed");
        let json_bytes = JsonLd.write(&model).expect("JSON-LD write failed");

        prop_assert!(!xmi_bytes.is_empty(), "XMI output is empty");
        prop_assert!(!yaml_bytes.is_empty(), "YAML output is empty");
        prop_assert!(!json_bytes.is_empty(), "JSON-LD output is empty");
    }

    /// Element count is preserved across all formats.
    #[test]
    fn element_count_preserved(model in arb_model_elements_only()) {
        let expected = model.element_count();

        for (name, format) in [("XMI", &Xmi as &dyn ModelFormat), ("YAML", &Yaml), ("JSON-LD", &JsonLd)] {
            let bytes = format.write(&model).expect(&format!("{name} write"));
            let rt = format.read(&bytes).expect(&format!("{name} read"));
            prop_assert_eq!(rt.element_count(), expected, "{} element count", name);
        }
    }

    /// Relationship count is preserved across YAML and JSON-LD.
    #[test]
    fn relationship_count_preserved(model in arb_model_with_relationships()) {
        let expected = model.relationship_count();

        for (name, format) in [("YAML", &Yaml as &dyn ModelFormat), ("JSON-LD", &JsonLd)] {
            let bytes = format.write(&model).expect(&format!("{name} write"));
            let rt = format.read(&bytes).expect(&format!("{name} read"));
            prop_assert_eq!(rt.relationship_count(), expected, "{} rel count", name);
        }
    }

    /// All element IDs are preserved across all formats.
    #[test]
    fn element_ids_preserved(model in arb_model_elements_only()) {
        let expected_ids: std::collections::HashSet<String> = model
            .elements
            .keys()
            .map(|id| id.as_str().to_string())
            .collect();

        for (name, format) in [("XMI", &Xmi as &dyn ModelFormat), ("YAML", &Yaml), ("JSON-LD", &JsonLd)] {
            let bytes = format.write(&model).expect(&format!("{name} write"));
            let rt = format.read(&bytes).expect(&format!("{name} read"));
            let rt_ids: std::collections::HashSet<String> = rt
                .elements
                .keys()
                .map(|id| id.as_str().to_string())
                .collect();
            prop_assert_eq!(&expected_ids, &rt_ids, "{} ID set mismatch", name);
        }
    }

    /// Writing the same model twice produces identical bytes.
    #[test]
    fn yaml_write_idempotent(model in arb_model_with_relationships()) {
        let bytes1 = Yaml.write(&model).expect("first write");
        let bytes2 = Yaml.write(&model).expect("second write");
        prop_assert_eq!(bytes1, bytes2, "YAML serialization not idempotent");
    }

    #[test]
    fn jsonld_write_idempotent(model in arb_model_with_relationships()) {
        let bytes1 = JsonLd.write(&model).expect("first write");
        let bytes2 = JsonLd.write(&model).expect("second write");
        prop_assert_eq!(bytes1, bytes2, "JSON-LD serialization not idempotent");
    }

    #[test]
    fn xmi_write_idempotent(model in arb_model_elements_only()) {
        let bytes1 = Xmi.write(&model).expect("first write");
        let bytes2 = Xmi.write(&model).expect("second write");
        prop_assert_eq!(bytes1, bytes2, "XMI serialization not idempotent");
    }
}
