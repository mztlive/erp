use super::list::{intersect_supplier_ids, qualification_constraint_kind, qualification_expiry_cutoff};
use super::{QualificationConstraintKind, SupplierQualificationHealthFilter};

#[test]
fn supplier_list_candidate_intersection_preserves_order_and_empty() {
    use erp_core::ids::SupplierAccountId;
    // 能力与资质双维度同时命中时取交集，且保留能力侧顺序。
    let capability_ids = Some(vec![
        SupplierAccountId::new("s-1"),
        SupplierAccountId::new("s-2"),
        SupplierAccountId::new("s-3"),
    ]);
    let qualification_ids = Some(vec![
        SupplierAccountId::new("s-2"),
        SupplierAccountId::new("s-3"),
        SupplierAccountId::new("s-4"),
    ]);
    assert_eq!(
        intersect_supplier_ids(capability_ids, qualification_ids)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["s-2".to_string(), "s-3".to_string()]
    );
    // 仅一侧约束时原样透传。
    assert_eq!(
        intersect_supplier_ids(Some(vec![SupplierAccountId::new("s-1")]), None)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["s-1".to_string()]
    );
    assert_eq!(
        intersect_supplier_ids(None, Some(vec![SupplierAccountId::new("s-9")]))
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["s-9".to_string()]
    );
    assert!(intersect_supplier_ids(None, None).is_none());
    assert_eq!(
        intersect_supplier_ids(
            Some(vec![SupplierAccountId::new("s-1")]),
            Some(vec![SupplierAccountId::new("s-9")])
        )
        .unwrap()
        .len(),
        0
    );
}

/// 资质约束分支覆盖类型、健康状态与未登记排除路径。
#[test]
fn qualification_constraint_kind_covers_all_branches() {
    use crate::entity::supplier::QualificationType;
    use QualificationConstraintKind::{Excluded, Included, Unconstrained};
    // 未筛选资质时无约束。
    assert_eq!(qualification_constraint_kind(&[], None), Unconstrained);
    // 仅按类型命中。
    assert_eq!(
        qualification_constraint_kind(&[QualificationType::FoodLicense], None),
        Included
    );
    // 各健康状态均走命中分支。
    for health in [
        SupplierQualificationHealthFilter::Valid,
        SupplierQualificationHealthFilter::Expiring30,
        SupplierQualificationHealthFilter::Expired,
    ] {
        assert_eq!(
            qualification_constraint_kind(&[QualificationType::FoodLicense], Some(health)),
            Included,
            "健康状态 {health:?} 应走命中分支"
        );
        assert_eq!(
            qualification_constraint_kind(&[], Some(health)),
            Included,
            "空类型下健康状态 {health:?} 仍应走命中分支"
        );
    }
    // 未登记资质走排除分支，命中集合为空。
    assert_eq!(
        qualification_constraint_kind(
            &[QualificationType::FoodLicense],
            Some(SupplierQualificationHealthFilter::NotRegistered)
        ),
        Excluded
    );
    assert_eq!(
        qualification_constraint_kind(&[], Some(SupplierQualificationHealthFilter::NotRegistered)),
        Excluded
    );
}

/// 到期窗口固定为起始日后第三十个自然日。
#[test]
fn supplier_list_expiry_cutoff_is_thirty_days_after_as_of() {
    assert_eq!(qualification_expiry_cutoff("2026-08-31").unwrap(), "2026-09-30");
    assert_eq!(qualification_expiry_cutoff("2026-01-31").unwrap(), "2026-03-02");
    assert_eq!(qualification_expiry_cutoff("2026-02-01").unwrap(), "2026-03-03");
    assert!(qualification_expiry_cutoff("not-a-date").is_err());
    assert!(qualification_expiry_cutoff("2026-13-01").is_err());
}
