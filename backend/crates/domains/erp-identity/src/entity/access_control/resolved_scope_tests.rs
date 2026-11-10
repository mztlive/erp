//! 范围维度的交集与身份隔离。
use super::*;

fn object(org: &str) -> ScopedObject<'_> {
    ScopedObject {
        owned: false,
        collaborating: false,
        historical_read_participant: false,
        org_unit_id: Some(org),
        settlement_party_id: None,
        warehouse_id: None,
    }
}

#[test]
fn missing_required_dimension_denies_and_independent_dimensions_intersect() {
    let only_org = ScopeClause { org_unit_ids: BTreeSet::from(["one".into()]), ..Default::default() };
    assert!(!only_org.complete(&[ScopeDimension::InternalOrg, ScopeDimension::Warehouse]));
    let both = ScopeClause { warehouse_ids: BTreeSet::from(["warehouse".into()]), ..only_org };
    assert!(both.complete(&[ScopeDimension::InternalOrg, ScopeDimension::Warehouse]));
    let mut facts = object("one");
    assert!(!both.covers(&facts));
    facts.warehouse_id = Some("warehouse");
    assert!(both.covers(&facts));
    facts.org_unit_id = Some("warehouse");
    assert!(!both.covers(&facts));
}
