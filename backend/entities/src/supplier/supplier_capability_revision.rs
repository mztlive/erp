//! `supplier_capability_revision`：供应商能力不可变修订（§6.2，P1 §2.2 快照字段）。
//!
//! 修订内联能力的结构化快照字段（能力代码、服务区域、负责人、履约说明、
//! 有效期与状态），由 P3 形成版本时填充（§4.4：后续基础资料修改不改变
//! 历史修订）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::supplier_capability::CapabilityStatus;

pub use crate::ids::{SupplierAccountId, SupplierCapabilityId, SupplierCapabilityRevisionId};

/// 服务区域引用最大长度。
const SERVICE_REGION_MAX_LEN: usize = 128;
/// 负责人标识最大长度。
const OWNER_USER_ID_MAX_LEN: usize = 128;
/// 履约说明最大长度。
const FULFILLMENT_NOTE_MAX_LEN: usize = 500;

/// 能力修订创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCapabilityRevisionData {
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 能力代码（修订内联快照）。
    pub capability_code: super::supplier_capability::CapabilityCode,
    /// 服务区域结构化引用（快照）。
    pub service_region: Option<String>,
    /// 负责人（快照）。
    pub owner_user_id: String,
    /// 履约说明（快照）。
    pub fulfillment_note: Option<String>,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 修订时点的能力状态（快照）。
    pub status: CapabilityStatus,
    /// 修订序号（同一能力内从 1 递增）。
    pub revision_no: u32,
}

/// 能力修订实体（不可变修订，§4.4：修订一经形成不得修改内容）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierCapabilityRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 能力代码。
    pub capability_code: super::supplier_capability::CapabilityCode,
    /// 服务区域结构化引用。
    pub service_region: Option<String>,
    /// 负责人。
    pub owner_user_id: String,
    /// 履约说明。
    pub fulfillment_note: Option<String>,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期。
    pub valid_to: Option<BusinessDate>,
    /// 修订时点的能力状态。
    pub status: CapabilityStatus,
}

impl SupplierCapabilityRevision {
    /// 创建能力修订。
    ///
    /// 完成负责人必填校验与全部文本字段的规范化（去首尾空白、长度
    /// 上限）；强制 `valid_to` 晚于 `valid_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierCapabilityRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的修订实体。
    ///
    /// # 错误
    /// 当负责人为空/超长、其他文本超长或生效区间倒挂时返回错误。
    pub fn new(id: SupplierCapabilityRevisionId, data: SupplierCapabilityRevisionData) -> Result<Self> {
        let service_region =
            normalize_optional_text(data.service_region, "服务区域", SERVICE_REGION_MAX_LEN)?;
        let owner_user_id = normalize_required_text(
            data.owner_user_id,
            "负责人不能为空",
            OWNER_USER_ID_MAX_LEN,
            "负责人标识过长",
        )?;
        let fulfillment_note =
            normalize_optional_text(data.fulfillment_note, "履约说明", FULFILLMENT_NOTE_MAX_LEN)?;
        ensure_window_valid(data.valid_from, data.valid_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            supplier_id: data.supplier_id,
            capability_code: data.capability_code,
            service_region,
            owner_user_id,
            fulfillment_note,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
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
    use super::{SupplierCapabilityRevision, SupplierCapabilityRevisionData};
    use crate::common::time::BusinessDate;
    use crate::ids::{SupplierAccountId, SupplierCapabilityRevisionId};
    use crate::supplier::supplier_capability::CapabilityCode;

    fn revision_data() -> SupplierCapabilityRevisionData {
        SupplierCapabilityRevisionData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            capability_code: CapabilityCode::Api,
            service_region: Some(" 华东 ".to_string()),
            owner_user_id: " buyer-1 ".to_string(),
            fulfillment_note: Some(" API 直连 ".to_string()),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            status: super::CapabilityStatus::Active,
            revision_no: 1,
        }
    }

    /// happy path：快照字段去空白，修订序号落库。
    #[test]
    fn new_trims_and_normalizes() {
        let revision =
            SupplierCapabilityRevision::new(SupplierCapabilityRevisionId::new("cap-rev-1"), revision_data())
                .unwrap();
        assert_eq!(revision.service_region.as_deref(), Some("华东"));
        assert_eq!(revision.owner_user_id, "buyer-1");
        assert_eq!(revision.fulfillment_note.as_deref(), Some("API 直连"));
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.capability_code, CapabilityCode::Api);
    }

    /// 失败路径：负责人为空/超长、区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_owner = SupplierCapabilityRevisionData {
            owner_user_id: "   ".to_string(),
            ..revision_data()
        };
        assert!(
            SupplierCapabilityRevision::new(SupplierCapabilityRevisionId::new("r"), blank_owner,).is_err()
        );

        let reversed = SupplierCapabilityRevisionData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..revision_data()
        };
        assert!(SupplierCapabilityRevision::new(SupplierCapabilityRevisionId::new("r"), reversed,).is_err());
    }

    /// 修订内联快照与稳定能力身份一致（快照字段不派生自其他域）。
    #[test]
    fn snapshot_is_self_contained() {
        let revision =
            SupplierCapabilityRevision::new(SupplierCapabilityRevisionId::new("cap-rev-2"), revision_data())
                .unwrap();
        assert_eq!(revision.supplier_id, SupplierAccountId::new("supplier-1"));
        assert_eq!(
            revision.capability_code.as_str(),
            "api",
            "快照字段携带稳定能力代码，不引用能力实体"
        );
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let revision =
            SupplierCapabilityRevision::new(SupplierCapabilityRevisionId::new("cap-rev-3"), revision_data())
                .unwrap();
        let roundtrip: SupplierCapabilityRevision =
            bson::from_document(bson::to_document(&revision).unwrap()).unwrap();
        assert_eq!(roundtrip, revision);
    }
}
