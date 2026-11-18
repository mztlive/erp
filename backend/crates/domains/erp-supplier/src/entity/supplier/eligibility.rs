//! 供应商能力修订合格判定（`PROC-E03`）。
//!
//! 集中“供应商-能力-修订在业务日是否合格”的纯领域规则；不依赖
//! 数据库、HTTP、全局时钟或全局 ID，仅对已加载事实做确定性校验。

use erp_core::common::time::BusinessDate;

use crate::entity::supplier::{
    CapabilityStatus, SupplierAccount, SupplierCapability, SupplierCapabilityRevision,
};

/// 供应商能力修订的不合格原因。
///
/// 用于区分 `PROC-E03` 要求的全部校验维度，便于上层保持
/// 既有 API 文案或做更细粒度的映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityEligibilityViolation {
    /// 供应商角色已停用。
    SupplierDisabled,
    /// 能力本身已停用。
    CapabilityDisabled,
    /// 能力修订快照所记录的状态为停用。
    RevisionDisabled,
    /// 修订归属与供应商或能力不一致。
    OwnershipMismatch,
    /// 修订不是能力当前生效版本。
    NotCurrentRevision,
    /// 业务日早于修订生效起始日。
    NotYetValid,
    /// 业务日已超过修订有效期。
    Expired,
}

impl std::fmt::Display for CapabilityEligibilityViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Self::SupplierDisabled => "供应商已停用，不能用于供给或采购",
            Self::CapabilityDisabled
            | Self::RevisionDisabled
            | Self::OwnershipMismatch
            | Self::NotCurrentRevision
            | Self::NotYetValid
            | Self::Expired => "供应商能力已停用、过期或版本已变化",
        };
        write!(f, "{msg}")
    }
}

impl std::error::Error for CapabilityEligibilityViolation {}

/// 供给或采购采用能力修订前，依次核对供应商、能力、修订启用状态，再校验归属、当前版本指针和有效期，返回
/// 首个 `CapabilityEligibilityViolation`。有效期含起止日，未设结束日视为长期有效。业务日由调用方提供；此
/// 处不查询、不修改实体，也不校验合同。
pub fn ensure_capability_qualified(
    supplier: &SupplierAccount,
    capability: &SupplierCapability,
    revision: &SupplierCapabilityRevision,
    on_date: BusinessDate,
) -> Result<(), CapabilityEligibilityViolation> {
    if !supplier.is_active() {
        return Err(CapabilityEligibilityViolation::SupplierDisabled);
    }
    if !capability.is_active() {
        return Err(CapabilityEligibilityViolation::CapabilityDisabled);
    }
    if revision.status != CapabilityStatus::Active {
        return Err(CapabilityEligibilityViolation::RevisionDisabled);
    }

    if revision.supplier_id.to_string() != supplier.base.id
        || revision.capability_code != capability.capability_code
        || capability.supplier_id.to_string() != supplier.base.id
    {
        return Err(CapabilityEligibilityViolation::OwnershipMismatch);
    }
    let is_current = capability.stable.current_revision_id.as_deref() == Some(revision.base.id.as_str());
    if !is_current {
        return Err(CapabilityEligibilityViolation::NotCurrentRevision);
    }

    if on_date < revision.valid_from {
        return Err(CapabilityEligibilityViolation::NotYetValid);
    }
    if let Some(valid_to) = revision.valid_to
        && on_date > valid_to
    {
        return Err(CapabilityEligibilityViolation::Expired);
    }
    Ok(())
}

/// 能力本身合格后核对其关联合同：没有关联不额外要求合同；有合同时至少一份须在业务日有效，未知日期不能证
/// 明有效。
pub fn ensure_linked_contracts_qualified(
    contracts: &[super::SupplierQualification],
    on_date: BusinessDate,
) -> erp_core::Result<()> {
    if contracts.is_empty() || contracts.iter().any(|contract| contract.is_valid_on(on_date)) {
        return Ok(());
    }
    Err(erp_core::Error::from("供应商该项能力的合同有效期未核实、未生效或已到期，请先维护有效合同"))
}

/// 供应商供给消费的公司商品类型事实；不依赖catalog实体。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferingProductKind {
    Physical,
    Virtual,
    OfflineService,
    Voucher,
}
/// 将供给商品类型映射为供应商必须具备的能力代码，供供给资格加载统一使用，不从供应商已有能力反推商品类型。
pub fn required_offering_capability(kind: OfferingProductKind) -> crate::entity::supplier::CapabilityCode {
    use crate::entity::supplier::CapabilityCode;
    match kind {
        OfferingProductKind::Physical => CapabilityCode::Physical,
        OfferingProductKind::Virtual | OfferingProductKind::Voucher => CapabilityCode::Virtual,
        OfferingProductKind::OfflineService => CapabilityCode::OfflineService,
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{PartyId, SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId};

    use super::{CapabilityEligibilityViolation, ensure_capability_qualified};
    use crate::entity::supplier::{
        CapabilityCode, CapabilityStatus, SupplierAccount, SupplierAccountData, SupplierAccountStatus,
        SupplierCapability, SupplierCapabilityData, SupplierCapabilityRevision,
        SupplierCapabilityRevisionData,
    };

    fn test_supplier(status: SupplierAccountStatus) -> SupplierAccount {
        SupplierAccount::new(
            SupplierAccountId::new("supplier-1"),
            SupplierAccountData {
                party_id: PartyId::new("party-1"),
                supplier_no: "S-001".to_string(),
                default_payment_term_id: None,
                current_commercial_profile_revision_id: None,
                maintainer_user_id: "buyer-1".to_string(),
                business_org_unit_id: "org-a".to_string(),
                status,
            },
            "admin-1",
        )
        .unwrap()
    }

    fn test_capability(
        supplier_id: &str,
        status: CapabilityStatus,
        current_revision_id: Option<&str>,
    ) -> SupplierCapability {
        let mut cap = SupplierCapability::new(
            SupplierCapabilityId::new("cap-1"),
            SupplierCapabilityData {
                supplier_id: SupplierAccountId::new(supplier_id),
                capability_code: CapabilityCode::Physical,
                service_region: None,
                owner_user_id: "buyer-1".to_string(),
                fulfillment_note: None,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                status,
            },
            "admin-1",
        )
        .unwrap();
        cap.stable.current_revision_id = current_revision_id.map(|s| s.to_string());
        cap
    }

    fn test_revision(
        supplier_id: &str,
        code: CapabilityCode,
        status: CapabilityStatus,
        valid_from: BusinessDate,
        valid_to: Option<BusinessDate>,
    ) -> SupplierCapabilityRevision {
        SupplierCapabilityRevision::new(
            SupplierCapabilityRevisionId::new("cap-rev-1"),
            SupplierCapabilityRevisionData {
                supplier_id: SupplierAccountId::new(supplier_id),
                capability_code: code,
                service_region: None,
                owner_user_id: "buyer-1".to_string(),
                fulfillment_note: None,
                valid_from,
                valid_to,
                status,
                revision_no: 1,
            },
        )
        .unwrap()
    }

    fn business_date(y: i32, m: u32, d: u32) -> BusinessDate {
        BusinessDate::from_ymd(y, m, d).unwrap()
    }

    #[test]
    fn qualified_when_all_active_and_within_window() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            Some(business_date(2026, 12, 31)),
        );
        assert!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 15))
                .is_ok()
        );
    }

    #[test]
    fn rejects_when_supplier_disabled() {
        let supplier = test_supplier(SupplierAccountStatus::Disabled);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            None,
        );
        assert_eq!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 1))
                .unwrap_err(),
            CapabilityEligibilityViolation::SupplierDisabled
        );
    }

    #[test]
    fn rejects_when_capability_disabled() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Disabled, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            None,
        );
        assert_eq!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 1))
                .unwrap_err(),
            CapabilityEligibilityViolation::CapabilityDisabled
        );
    }

    #[test]
    fn rejects_when_revision_disabled() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Disabled,
            business_date(2026, 1, 1),
            None,
        );
        assert_eq!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 1))
                .unwrap_err(),
            CapabilityEligibilityViolation::RevisionDisabled
        );
    }

    #[test]
    fn rejects_when_ownership_mismatch() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        // revision belongs to different supplier
        let revision = test_revision(
            "supplier-2",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            None,
        );
        assert_eq!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 1))
                .unwrap_err(),
            CapabilityEligibilityViolation::OwnershipMismatch
        );
    }

    #[test]
    fn rejects_when_capability_code_mismatch() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Api,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            None,
        );
        assert_eq!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 1))
                .unwrap_err(),
            CapabilityEligibilityViolation::OwnershipMismatch
        );
    }

    #[test]
    fn rejects_when_not_current_revision() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        // capability points to different revision
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-2"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            None,
        );
        assert_eq!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 1))
                .unwrap_err(),
            CapabilityEligibilityViolation::NotCurrentRevision
        );
    }

    #[test]
    fn rejects_when_current_revision_is_none() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, None);
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            None,
        );
        assert_eq!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 1))
                .unwrap_err(),
            CapabilityEligibilityViolation::NotCurrentRevision
        );
    }

    #[test]
    fn accepts_on_valid_from_boundary() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 10),
            Some(business_date(2026, 12, 31)),
        );
        assert!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 1, 10))
                .is_ok()
        );
    }

    #[test]
    fn accepts_on_valid_to_boundary() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            Some(business_date(2026, 6, 15)),
        );
        assert!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 15))
                .is_ok()
        );
    }

    #[test]
    fn rejects_before_valid_from() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 6, 15),
            Some(business_date(2026, 12, 31)),
        );
        assert_eq!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 14))
                .unwrap_err(),
            CapabilityEligibilityViolation::NotYetValid
        );
    }

    #[test]
    fn rejects_after_valid_to() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            Some(business_date(2026, 6, 15)),
        );
        assert_eq!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 6, 16))
                .unwrap_err(),
            CapabilityEligibilityViolation::Expired
        );
    }

    #[test]
    fn accepts_long_term_valid_to_none() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            None,
        );
        assert!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2030, 1, 1)).is_ok()
        );
    }

    #[test]
    fn rejects_when_expired_by_one_day() {
        let supplier = test_supplier(SupplierAccountStatus::Active);
        let capability = test_capability("supplier-1", CapabilityStatus::Active, Some("cap-rev-1"));
        let revision = test_revision(
            "supplier-1",
            CapabilityCode::Physical,
            CapabilityStatus::Active,
            business_date(2026, 1, 1),
            Some(business_date(2026, 3, 31)),
        );
        // on_date == valid_to is ok, +1 day is expired
        assert!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 3, 31))
                .is_ok()
        );
        assert_eq!(
            ensure_capability_qualified(&supplier, &capability, &revision, business_date(2026, 4, 1))
                .unwrap_err(),
            CapabilityEligibilityViolation::Expired
        );
    }
}

#[cfg(test)]
mod offering_kind_tests {
    use super::{OfferingProductKind, required_offering_capability};
    use crate::entity::supplier::CapabilityCode;
    #[test]
    fn offering_kind_preserves_all_four_mappings() {
        for (kind, code) in [
            (OfferingProductKind::Physical, CapabilityCode::Physical),
            (OfferingProductKind::Virtual, CapabilityCode::Virtual),
            (OfferingProductKind::OfflineService, CapabilityCode::OfflineService),
            (OfferingProductKind::Voucher, CapabilityCode::Virtual),
        ] {
            assert_eq!(required_offering_capability(kind), code);
        }
    }
}
