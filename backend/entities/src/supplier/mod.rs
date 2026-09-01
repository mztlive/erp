//! 域 D09 `supplier`：supplier_account、supplier_commercial_profile_revision、
//! supplier_capability(+_revision)、supplier_qualification(+_revision)、
//! supplier_qualification_capability、supplier_rating_revision（页面：W14）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与唯一约束见数据模型 §6.2；公共字段归属按 §4.3 判定：
//! - `supplier_account` / `supplier_capability` / `supplier_qualification`
//!   是「稳定基础资料」→ 组合 [`crate::common::StableBase`]；
//! - `supplier_commercial_profile_revision` / `supplier_capability_revision` /
//!   `supplier_qualification_revision` / `supplier_rating_revision` 是不可变修订
//!   → 组合 [`crate::common::RevisionBase`]，快照字段按 §2.2 / §4.4 内联
//!   （付款条件、能力、资质证照、评分等，P3 填充）；
//! - `supplier_qualification_capability` 是资质 ↔ 能力的纯关联行（§6.2）；
//! - 跨聚合校验（资质失效不得用于新供给/采购单等）留给 P3，注释标注条目号。

pub mod business_category;
pub mod eligibility;
pub mod payment_term;
pub mod profile_change;
pub mod supplier_account;
pub mod supplier_capability;
pub mod supplier_capability_revision;
pub mod supplier_commercial_profile_revision;
pub mod supplier_profile_command;
pub mod supplier_qualification;
pub mod supplier_qualification_capability;
pub mod supplier_qualification_revision;
pub mod supplier_rating_revision;

pub use crate::ids::{
    SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId,
    SupplierCommercialProfileRevisionId, SupplierQualificationCapabilityId, SupplierQualificationId,
    SupplierQualificationRevisionId, SupplierRatingRevisionId,
};
pub use business_category::{
    normalize_business_category, split_encoded_payment_term_snapshot, PaymentTermSnapshotParts,
};
pub use payment_term::{SettlementMode, SupplierPaymentTerm};
pub use supplier_account::{
    SupplierAccount, SupplierAccountData, SupplierAccountStatus, SupplierAccountUpdate,
    SupplierProfileUpdateViolation,
};
pub use supplier_capability::{
    CapabilityCode, CapabilityStatus, SupplierCapability, SupplierCapabilityData, SupplierCapabilityUpdate,
};
pub use supplier_capability_revision::{SupplierCapabilityRevision, SupplierCapabilityRevisionData};
pub use supplier_commercial_profile_revision::{
    InvoiceType, ReconciliationCycle, SupplierCommercialProfileRevision,
    SupplierCommercialProfileRevisionData,
};
pub use supplier_profile_command::{SupplierProfileCommand, SupplierProfileCommandData};
pub use supplier_qualification::{
    qualification_identity_key, QualificationAttachmentSensitivity, QualificationStatus, QualificationType,
    SupplierQualification, SupplierQualificationData, SupplierQualificationUpdate,
};
pub use supplier_qualification_capability::{
    SupplierQualificationCapability, SupplierQualificationCapabilityData,
};
pub use supplier_qualification_revision::{SupplierQualificationRevision, SupplierQualificationRevisionData};
pub use supplier_rating_revision::{SupplierRating, SupplierRatingRevision, SupplierRatingRevisionData};

/// 返回供应商追加式修订序列的下一号。
///
/// # 参数
/// * `revision_numbers` - 已有修订号迭代器
///
/// # 返回
/// 无历史时返回 `1`，否则返回最大修订号加一。
///
/// # 错误
/// 当前最大修订号已达到 `u32::MAX` 时返回业务错误。
pub fn next_supplier_revision_no(revision_numbers: impl IntoIterator<Item = u32>) -> crate::Result<u32> {
    revision_numbers
        .into_iter()
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| crate::Error::from("供应商资料修订号已达上限"))
}

/// 根资料命令中的资质选择项借用视图。
#[derive(Debug, Clone, Copy)]
pub struct SupplierQualificationSelection<'a> {
    /// 资质类型。
    pub qualification_type: QualificationType,
    /// 证书编号。
    pub certificate_no: &'a str,
    /// 该资质覆盖的能力代码。
    pub capability_codes: &'a [CapabilityCode],
}

/// 校验根资料命令中的能力与资质选择关系。
///
/// # 参数
/// * `capability_codes` - 根资料勾选的供应商能力
/// * `qualifications` - 根资料提交的资质选择项
///
/// # 返回
/// 能力不重复、资质身份不重复且所有资质仅引用已勾选能力时返回 `Ok(())`。
///
/// # 错误
/// 能力重复、同类证书编号重复或资质引用未勾选能力时返回业务错误。
pub fn validate_profile_selection(
    capability_codes: &[CapabilityCode],
    qualifications: &[SupplierQualificationSelection<'_>],
) -> crate::Result<()> {
    let capability_set: std::collections::HashSet<CapabilityCode> =
        capability_codes.iter().copied().collect();
    if capability_set.len() != capability_codes.len() {
        return Err(crate::Error::from("供应商能力不能重复"));
    }
    let mut qualification_keys = std::collections::HashSet::new();
    for qualification in qualifications {
        let key = qualification_identity_key(qualification.qualification_type, qualification.certificate_no);
        if !qualification_keys.insert(key) {
            return Err(crate::Error::from("同类资质编号不能重复"));
        }
        if qualification
            .capability_codes
            .iter()
            .any(|code| !capability_set.contains(code))
        {
            return Err(crate::Error::from("资质引用了未启用的供应商能力"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod profile_selection_tests {
    use super::{
        next_supplier_revision_no, validate_profile_selection, CapabilityCode, QualificationType,
        SupplierQualificationSelection,
    };

    /// 修订序列从一开始、按最大值推进并拒绝溢出。
    #[test]
    fn revision_sequence_starts_at_one_and_advances_maximum() {
        assert_eq!(next_supplier_revision_no([]).unwrap(), 1);
        assert_eq!(next_supplier_revision_no([1, 3, 2]).unwrap(), 4);
        assert!(next_supplier_revision_no([u32::MAX]).is_err());
    }

    /// 唯一能力及其覆盖资质可以通过根资料校验。
    #[test]
    fn profile_selection_accepts_unique_covered_qualifications() {
        let qualification_codes = [CapabilityCode::Physical];
        let qualifications = [SupplierQualificationSelection {
            qualification_type: QualificationType::FoodLicense,
            certificate_no: " FOOD-1 ",
            capability_codes: &qualification_codes,
        }];
        assert!(validate_profile_selection(&[CapabilityCode::Physical], &qualifications).is_ok());
    }

    /// 重复能力、重复资质身份和未勾选能力引用均被拒绝。
    #[test]
    fn profile_selection_rejects_duplicates_and_unselected_capabilities() {
        assert!(
            validate_profile_selection(&[CapabilityCode::Physical, CapabilityCode::Physical], &[],).is_err()
        );
        let qualification_codes = [CapabilityCode::Api];
        let qualifications = [SupplierQualificationSelection {
            qualification_type: QualificationType::Contract,
            certificate_no: "HT-1",
            capability_codes: &qualification_codes,
        }];
        assert!(validate_profile_selection(&[CapabilityCode::Physical], &qualifications).is_err());

        let duplicate = [
            SupplierQualificationSelection {
                qualification_type: QualificationType::Contract,
                certificate_no: " HT-1 ",
                capability_codes: &[],
            },
            SupplierQualificationSelection {
                qualification_type: QualificationType::Contract,
                certificate_no: "HT-1",
                capability_codes: &[],
            },
        ];
        assert!(validate_profile_selection(&[], &duplicate).is_err());
    }
}
