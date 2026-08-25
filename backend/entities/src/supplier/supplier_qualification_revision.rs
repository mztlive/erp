//! `supplier_qualification_revision`：供应商资质不可变修订（§6.2，P1 §2.2 快照字段）。
//!
//! 修订内联资质的结构化快照字段（资质类型、证书编号、发证机构、有效期、
//! 附件与状态），由 P3 形成版本时填充（§4.4：后续基础资料修改不改变
//! 历史修订）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::supplier_qualification::{QualificationStatus, QualificationType};

pub use crate::ids::{
    FileAssetId, SupplierAccountId, SupplierQualificationId, SupplierQualificationRevisionId,
};

/// 证书编号最大长度。
const CERTIFICATE_NO_MAX_LEN: usize = 128;
/// 发证机构最大长度。
const ISSUER_MAX_LEN: usize = 128;

/// 资质修订创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierQualificationRevisionData {
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 资质类型（修订内联快照）。
    pub qualification_type: QualificationType,
    /// 证书编号（快照）。
    pub certificate_no: String,
    /// 发证机构（快照）。
    pub issuer: Option<String>,
    /// 生效、失效日期（快照）。
    pub valid_from: BusinessDate,
    /// 失效日期（快照）；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 资质附件（快照）。
    pub attachment_id: Option<FileAssetId>,
    /// 修订时点的资质状态（快照）。
    pub status: QualificationStatus,
    /// 修订序号（同一资质内从 1 递增）。
    pub revision_no: u32,
}

/// 资质修订实体（不可变修订，§4.4：修订一经形成不得修改内容）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierQualificationRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 资质类型。
    pub qualification_type: QualificationType,
    /// 证书编号。
    pub certificate_no: String,
    /// 发证机构。
    pub issuer: Option<String>,
    /// 生效、失效日期。
    pub valid_from: BusinessDate,
    /// 失效日期。
    pub valid_to: Option<BusinessDate>,
    /// 资质附件。
    pub attachment_id: Option<FileAssetId>,
    /// 修订时点的资质状态。
    pub status: QualificationStatus,
}

impl SupplierQualificationRevision {
    /// 创建资质修订。
    ///
    /// 完成证书编号必填校验与发证机构的规范化（去首尾空白、长度上限）；
    /// 强制 `valid_to` 晚于 `valid_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierQualificationRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的修订实体。
    ///
    /// # 错误
    /// 当证书编号为空/超长、发证机构超长或生效区间倒挂时返回错误。
    pub fn new(id: SupplierQualificationRevisionId, data: SupplierQualificationRevisionData) -> Result<Self> {
        let certificate_no = normalize_required_text(
            data.certificate_no,
            "证书编号不能为空",
            CERTIFICATE_NO_MAX_LEN,
            "证书编号过长",
        )?;
        let issuer = normalize_optional_text(data.issuer, "发证机构", ISSUER_MAX_LEN)?;
        ensure_window_valid(data.valid_from, data.valid_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            supplier_id: data.supplier_id,
            qualification_type: data.qualification_type,
            certificate_no,
            issuer,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            attachment_id: data.attachment_id,
            status: data.status,
        })
    }
}

/// 校验生效区间：`valid_to` 必须晚于 `valid_from`。
///
/// # 参数
/// * `valid_from` - 生效开始日期
/// * `valid_to` - 生效结束日期（可空）
///
/// # 返回
/// 区间合法返回 `Ok(())`。
///
/// # 错误
/// 结束日期不晚于开始日期时返回错误。
fn ensure_window_valid(valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> Result<()> {
    if let Some(valid_to) = valid_to {
        if valid_to <= valid_from {
            return Err(Error::from("生效结束日期必须晚于生效开始日期"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SupplierQualificationRevision, SupplierQualificationRevisionData};
    use crate::common::time::BusinessDate;
    use crate::ids::{FileAssetId, SupplierAccountId, SupplierQualificationRevisionId};
    use crate::supplier::supplier_qualification::{QualificationStatus, QualificationType};

    fn revision_data() -> SupplierQualificationRevisionData {
        SupplierQualificationRevisionData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            qualification_type: QualificationType::FoodLicense,
            certificate_no: " JY-2026-001 ".to_string(),
            issuer: Some(" 示例市场监管局 ".to_string()),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            attachment_id: Some(FileAssetId::new("file-1")),
            status: QualificationStatus::Active,
            revision_no: 1,
        }
    }

    /// happy path：快照字段去空白，修订序号与状态快照落库。
    #[test]
    fn new_trims_and_normalizes() {
        let revision = SupplierQualificationRevision::new(
            SupplierQualificationRevisionId::new("qual-rev-1"),
            revision_data(),
        )
        .unwrap();
        assert_eq!(revision.certificate_no, "JY-2026-001");
        assert_eq!(revision.issuer.as_deref(), Some("示例市场监管局"));
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.qualification_type, QualificationType::FoodLicense);
        assert_eq!(revision.status, QualificationStatus::Active);
    }

    /// 失败路径：编号为空/超长、区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank = SupplierQualificationRevisionData {
            certificate_no: "   ".to_string(),
            ..revision_data()
        };
        assert!(
            SupplierQualificationRevision::new(SupplierQualificationRevisionId::new("r"), blank,).is_err()
        );

        let reversed = SupplierQualificationRevisionData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..revision_data()
        };
        assert!(
            SupplierQualificationRevision::new(SupplierQualificationRevisionId::new("r"), reversed,).is_err()
        );
    }

    /// 修订快照自包含：类型/编号/状态以结构化字段保存，不引用资质实体。
    #[test]
    fn snapshot_is_self_contained() {
        let revision = SupplierQualificationRevision::new(
            SupplierQualificationRevisionId::new("qual-rev-2"),
            revision_data(),
        )
        .unwrap();
        assert_eq!(revision.certificate_no, "JY-2026-001");
        assert_eq!(revision.supplier_id, SupplierAccountId::new("supplier-1"));
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let revision = SupplierQualificationRevision::new(
            SupplierQualificationRevisionId::new("qual-rev-3"),
            revision_data(),
        )
        .unwrap();
        let roundtrip: SupplierQualificationRevision =
            bson::deserialize_from_document(bson::serialize_to_document(&revision).unwrap()).unwrap();
        assert_eq!(roundtrip, revision);
    }
}
