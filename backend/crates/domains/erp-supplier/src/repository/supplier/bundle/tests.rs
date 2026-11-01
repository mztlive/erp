use super::list::{
    IncludedQualificationHealth, included_qualification_health, qualification_constraint_kind,
    qualification_expiry_cutoff,
};
use super::{QualificationConstraintKind, SupplierQualificationHealthFilter};

/// 资质约束分支覆盖类型、健康状态与未登记排除路径。
#[test]
fn qualification_constraint_kind_covers_all_branches() {
    use QualificationConstraintKind::{Excluded, Included, Unconstrained};

    use crate::entity::supplier::QualificationType;
    // 未筛选资质时无约束。
    assert_eq!(qualification_constraint_kind(&[], None), Unconstrained);
    // 仅按类型命中。
    assert_eq!(qualification_constraint_kind(&[QualificationType::FoodLicense], None), Included);
    // 各健康状态均走命中分支。
    for health in [
        SupplierQualificationHealthFilter::Valid,
        SupplierQualificationHealthFilter::Expiring30,
        SupplierQualificationHealthFilter::Unverified,
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

/// 命中集合健康状态不含 `NotRegistered`，未登记仍由排除集路径处理。
#[test]
fn included_qualification_health_omits_not_registered() {
    assert_eq!(included_qualification_health(None), Some(IncludedQualificationHealth::ByType));
    assert_eq!(
        included_qualification_health(Some(SupplierQualificationHealthFilter::Unverified)),
        Some(IncludedQualificationHealth::Unverified)
    );
    assert_eq!(
        included_qualification_health(Some(SupplierQualificationHealthFilter::Valid)),
        Some(IncludedQualificationHealth::Valid)
    );
    assert_eq!(
        included_qualification_health(Some(SupplierQualificationHealthFilter::Expiring30)),
        Some(IncludedQualificationHealth::Expiring30)
    );
    assert_eq!(
        included_qualification_health(Some(SupplierQualificationHealthFilter::Expired)),
        Some(IncludedQualificationHealth::Expired)
    );
    assert_eq!(included_qualification_health(Some(SupplierQualificationHealthFilter::NotRegistered)), None);
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
