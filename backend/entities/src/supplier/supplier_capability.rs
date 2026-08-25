//! `supplier_capability`：供应商能力（数据模型 §6.2，页面：W14）。
//!
//! 一家供应商维护一种或多种能力（多选）；能力失效（资质失效）后不得
//! 用于为公司 SKU 新建或延续有效供给（跨聚合约束，P3 事务校验，
//! §6.2 必需约束，条目 P3-§6.2-qualification-gate）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::field_update::FieldUpdate;
use crate::validation::{normalize_optional_text, normalize_required_text};

pub use crate::ids::{SupplierAccountId, SupplierCapabilityId};

/// 服务区域引用最大长度。
const SERVICE_REGION_MAX_LEN: usize = 128;
/// 负责人标识最大长度。
const OWNER_USER_ID_MAX_LEN: usize = 128;
/// 履约说明最大长度。
const FULFILLMENT_NOTE_MAX_LEN: usize = 500;

/// 能力代码（§6.2：实物、虚拟、线下服务、API、印刷；固定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityCode {
    /// 实物商品。
    Physical,
    /// 虚拟商品。
    Virtual,
    /// 线下服务。
    OfflineService,
    /// API。
    Api,
    /// 印刷。
    Printing,
}

impl CapabilityCode {
    /// 返回能力的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Physical => "实物商品",
            Self::Virtual => "虚拟商品",
            Self::OfflineService => "线下服务",
            Self::Api => "API",
            Self::Printing => "印刷",
        }
    }

    /// 返回能力的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Physical => "physical",
            Self::Virtual => "virtual",
            Self::OfflineService => "offline_service",
            Self::Api => "api",
            Self::Printing => "printing",
        }
    }
}

/// 能力启停状态（§6.2：启用/停用；对称状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

impl CapabilityStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "启用",
            Self::Disabled => "停用",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    /// 判断是否处于启用状态。
    ///
    /// # 返回
    /// 处于 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

impl DocumentState for CapabilityStatus {
    /// 返回合法后继：启用 ⇄ 停用。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Disabled],
            Self::Disabled => &[Self::Active],
        }
    }
}

/// 能力创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCapabilityData {
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 能力代码（同一供应商内唯一，创建后不可修改）。
    pub capability_code: CapabilityCode,
    /// 服务区域结构化引用。
    pub service_region: Option<String>,
    /// 负责人。
    pub owner_user_id: String,
    /// 履约说明。
    pub fulfillment_note: Option<String>,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 启停状态。
    pub status: CapabilityStatus,
}

/// 能力更新数据。
///
/// `supplier_id` 与 `capability_code` 是稳定身份（身份组合），不允许
/// 在通用更新中修改；同一能力有效区间不得重叠（跨行约束由 P3 校验）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCapabilityUpdate {
    /// 服务区域更新意图。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub service_region: FieldUpdate<String>,
    /// 负责人；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    /// 履约说明更新意图。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub fulfillment_note: FieldUpdate<String>,
    /// 生效结束日期更新意图（`Set` 时校验晚于 `valid_from`）。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub valid_to: FieldUpdate<BusinessDate>,
    /// 启停状态；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<CapabilityStatus>,
}

/// 供应商能力实体（稳定基础资料，§6.2）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SupplierCapability {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<CapabilityStatus>,
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 能力代码。
    pub capability_code: CapabilityCode,
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
}

impl PartialEq for SupplierCapability {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.supplier_id == other.supplier_id
            && self.capability_code == other.capability_code
            && self.service_region == other.service_region
            && self.owner_user_id == other.owner_user_id
            && self.fulfillment_note == other.fulfillment_note
            && self.valid_from == other.valid_from
            && self.valid_to == other.valid_to
    }
}

impl Eq for SupplierCapability {}

impl SupplierCapability {
    /// 创建供应商能力。
    ///
    /// 完成负责人必填校验与全部文本字段的规范化（去首尾空白、长度
    /// 上限）；强制 `valid_to` 晚于 `valid_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierCapabilityId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的能力实体。
    ///
    /// # 错误
    /// 当负责人为空/超长、其他文本超长或生效区间倒挂时返回错误。
    pub fn new(
        id: SupplierCapabilityId,
        data: SupplierCapabilityData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
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
            stable: StableBase::new(data.status, created_by),
            supplier_id: data.supplier_id,
            capability_code: data.capability_code,
            service_region,
            owner_user_id,
            fulfillment_note,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
        })
    }

    /// 更新供应商能力。
    ///
    /// `supplier_id` 与 `capability_code` 是稳定身份，不允许在通用更新
    /// 中修改；状态迁移按固定状态机校验（§13.3）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当文本超长、`valid_to` 倒挂或状态迁移非法时返回错误。
    pub fn update(&mut self, update: SupplierCapabilityUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.apply_service_region(update.service_region)?;
        self.apply_owner(update.owner_user_id)?;
        self.apply_fulfillment_note(update.fulfillment_note)?;
        self.apply_valid_to(update.valid_to)?;
        self.apply_status(update.status)?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断能力是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    /// 应用服务区域更新。
    ///
    /// # 参数
    /// * `update` - 区域更新意图
    ///
    /// # 错误
    /// 当区域超长时返回错误。
    fn apply_service_region(&mut self, update: FieldUpdate<String>) -> Result<()> {
        match update {
            FieldUpdate::Unchanged => {}
            FieldUpdate::Clear => self.service_region = None,
            FieldUpdate::Set(value) => {
                self.service_region =
                    normalize_optional_text(Some(value), "服务区域", SERVICE_REGION_MAX_LEN)?
            }
        }
        Ok(())
    }

    /// 应用负责人更新。
    ///
    /// # 参数
    /// * `owner` - 可选负责人
    ///
    /// # 错误
    /// 当负责人为空或超长时返回错误。
    fn apply_owner(&mut self, owner: Option<String>) -> Result<()> {
        if let Some(owner) = owner {
            self.owner_user_id =
                normalize_required_text(owner, "负责人不能为空", OWNER_USER_ID_MAX_LEN, "负责人标识过长")?;
        }
        Ok(())
    }

    /// 应用履约说明更新。
    ///
    /// # 参数
    /// * `update` - 履约说明更新意图
    ///
    /// # 错误
    /// 当说明超长时返回错误。
    fn apply_fulfillment_note(&mut self, update: FieldUpdate<String>) -> Result<()> {
        match update {
            FieldUpdate::Unchanged => {}
            FieldUpdate::Clear => self.fulfillment_note = None,
            FieldUpdate::Set(value) => {
                self.fulfillment_note =
                    normalize_optional_text(Some(value), "履约说明", FULFILLMENT_NOTE_MAX_LEN)?
            }
        }
        Ok(())
    }

    /// 应用生效结束日期更新。
    ///
    /// # 参数
    /// * `update` - 结束日期更新意图
    ///
    /// # 错误
    /// 当结束日期不晚于开始日期时返回错误。
    fn apply_valid_to(&mut self, update: FieldUpdate<BusinessDate>) -> Result<()> {
        if let Some(valid_to) = update.into_option() {
            ensure_window_valid(self.valid_from, Some(valid_to))?;
            self.valid_to = Some(valid_to);
        }
        Ok(())
    }

    /// 应用状态更新。
    ///
    /// # 参数
    /// * `status` - 可选目标状态
    ///
    /// # 错误
    /// 目标状态不在固定状态机后继中时返回错误。
    fn apply_status(&mut self, status: Option<CapabilityStatus>) -> Result<()> {
        if let Some(to) = status {
            ensure_transition(self.stable.status, to)?;
            self.stable.status = to;
        }
        Ok(())
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
    use super::{
        CapabilityCode, CapabilityStatus, SupplierCapability, SupplierCapabilityData,
        SupplierCapabilityUpdate,
    };
    use crate::common::state::assert_adjacency_closed;
    use crate::common::time::BusinessDate;
    use crate::field_update::FieldUpdate;
    use crate::ids::{SupplierAccountId, SupplierCapabilityId};

    fn capability_data() -> SupplierCapabilityData {
        SupplierCapabilityData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            capability_code: CapabilityCode::Physical,
            service_region: Some(" 华东 ".to_string()),
            owner_user_id: " buyer-1 ".to_string(),
            fulfillment_note: Some(" 常规履约 ".to_string()),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            status: CapabilityStatus::Active,
        }
    }

    /// happy path：文本去空白，能力代码与状态落库。
    #[test]
    fn new_trims_and_normalizes() {
        let capability =
            SupplierCapability::new(SupplierCapabilityId::new("cap-1"), capability_data(), "admin-1")
                .unwrap();
        assert_eq!(capability.service_region.as_deref(), Some("华东"));
        assert_eq!(capability.owner_user_id, "buyer-1");
        assert_eq!(capability.fulfillment_note.as_deref(), Some("常规履约"));
        assert_eq!(capability.capability_code, CapabilityCode::Physical);
        assert!(capability.is_active());
    }

    /// 失败路径：负责人为空/超长、区域超长、区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_owner = SupplierCapabilityData {
            owner_user_id: "   ".to_string(),
            ..capability_data()
        };
        assert!(SupplierCapability::new(SupplierCapabilityId::new("c"), blank_owner, "admin-1").is_err());

        let overlong_region = SupplierCapabilityData {
            service_region: Some("r".repeat(129)),
            ..capability_data()
        };
        assert!(SupplierCapability::new(SupplierCapabilityId::new("c"), overlong_region, "admin-1").is_err());

        let reversed = SupplierCapabilityData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..capability_data()
        };
        assert!(SupplierCapability::new(SupplierCapabilityId::new("c"), reversed, "admin-1").is_err());
    }

    /// 状态机：邻接矩阵闭包完整，合法迁移通过。
    #[test]
    fn status_transitions_follow_fixed_matrix() {
        assert_adjacency_closed(&[CapabilityStatus::Active, CapabilityStatus::Disabled]);

        let mut capability =
            SupplierCapability::new(SupplierCapabilityId::new("cap-2"), capability_data(), "admin-1")
                .unwrap();
        capability
            .update(
                SupplierCapabilityUpdate {
                    service_region: FieldUpdate::Clear,
                    owner_user_id: Some("buyer-2".to_string()),
                    fulfillment_note: FieldUpdate::Unchanged,
                    valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2026, 6, 30).unwrap()),
                    status: Some(CapabilityStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        assert!(!capability.is_active());
        assert_eq!(capability.service_region, None);
        assert_eq!(capability.owner_user_id, "buyer-2");
        assert_eq!(
            capability.valid_to,
            Some(BusinessDate::from_ymd(2026, 6, 30).unwrap())
        );
        assert_eq!(capability.stable.updated_by, "admin-2");
    }

    /// 更新：能力代码与供应商不可修改（不在更新面）。
    #[test]
    fn update_keeps_stable_identity() {
        let mut capability =
            SupplierCapability::new(SupplierCapabilityId::new("cap-3"), capability_data(), "admin-1")
                .unwrap();
        capability
            .update(
                SupplierCapabilityUpdate {
                    service_region: FieldUpdate::Unchanged,
                    owner_user_id: None,
                    fulfillment_note: FieldUpdate::Unchanged,
                    valid_to: FieldUpdate::Unchanged,
                    status: None,
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(capability.capability_code, CapabilityCode::Physical);
        assert_eq!(capability.supplier_id, SupplierAccountId::new("supplier-1"));
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let capability =
            SupplierCapability::new(SupplierCapabilityId::new("cap-4"), capability_data(), "admin-1")
                .unwrap();
        let roundtrip: SupplierCapability =
            bson::deserialize_from_document(bson::serialize_to_document(&capability).unwrap()).unwrap();
        assert_eq!(roundtrip, capability);
    }
}
