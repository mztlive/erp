//! 供应商能力修订合格判定（`PROC-E03`）。
//!
//! 集中“供应商-能力-修订在业务日是否合格”的纯领域规则；不依赖
//! 数据库、HTTP、全局时钟或全局 ID，仅对已加载事实做确定性校验。

use crate::common::time::BusinessDate;
use crate::supplier::{CapabilityStatus, SupplierAccount, SupplierCapability, SupplierCapabilityRevision};

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

/// 校验指定能力修订在业务日是否合格。
///
/// 依次校验供应商启用状态、能力启用状态、修订启用状态、修订归属
/// （`revision.supplier_id` 必须等于供应商主键且修订能力代码必须与能力
/// 代码一致）、当前版本指针及有效期窗口。有效期为闭区间
/// `valid_from <= on_date <= valid_to`；`valid_to == None` 视为长期有效。
///
/// # 参数
/// * `supplier` - 供应商角色实体
/// * `capability` - 供应商能力实体
/// * `revision` - 待校验的能力修订快照
/// * `on_date` - 业务自然日（由 Service 显式注入，不读取全局时钟）
///
/// # 返回
/// 全部校验通过返回 `Ok(())`，否则返回首个命中的 [`CapabilityEligibilityViolation`]。
///
/// # 错误
/// * `SupplierDisabled` - 供应商已停用
/// * `CapabilityDisabled` - 能力已停用
/// * `RevisionDisabled` - 修订状态非 `Active`
/// * `OwnershipMismatch` - 修订 `supplier_id` 与供应商主键不一致，或修订能力代码与能力代码不一致
/// * `NotCurrentRevision` - 能力的 `current_revision_id` 与修订主键不一致
/// * `NotYetValid` - 业务日早于 `valid_from`
/// * `Expired` - 业务日已超过 `valid_to`
///
/// # 约束
/// * 纯内存判定，不触及 MongoDB、HTTP、时钟或密钥
/// * 不分配新 ID，不改变任何实体状态
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
    if let Some(valid_to) = revision.valid_to {
        if on_date > valid_to {
            return Err(CapabilityEligibilityViolation::Expired);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_capability_qualified, CapabilityEligibilityViolation};
    use crate::common::time::BusinessDate;
    use crate::ids::PartyId;
    use crate::ids::{SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId};
    use crate::supplier::{
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
