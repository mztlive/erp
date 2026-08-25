//! `supplier_qualification`：供应商资质（数据模型 §6.2，页面：W14）。
//!
//! 资质失效后对应供应商能力不得用于新建或延续有效供给、不得用于采购单
//! （跨聚合约束，P3 事务校验，§4.5 与 §6.2 必需约束，条目
//! P3-§6.2-qualification-gate）；资质到期预警由 P3 定时任务生成。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::field_update::FieldUpdate;
use crate::validation::{normalize_optional_text, normalize_required_text};

pub use crate::ids::{FileAssetId, SupplierAccountId, SupplierQualificationId};

/// 证书编号最大长度。
const CERTIFICATE_NO_MAX_LEN: usize = 128;
/// 发证机构最大长度。
const ISSUER_MAX_LEN: usize = 128;

/// 资质类型（§6.2：资质证照、合同、授权书、食品经营许可证、法人身份证等）。
///
/// 合同、授权书、食品经营许可证和法人身份证以受控 `qualification_type`
/// 表达，附件走受控下载并记录访问审计（§6.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationType {
    /// 资质证照。
    Certificate,
    /// 合同（编号使用合同编号）。
    Contract,
    /// 授权书（编号使用授权编号）。
    Authorization,
    /// 食品经营许可证。
    FoodLicense,
    /// 法人身份证。
    LegalPersonId,
}

impl QualificationType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Certificate => "资质证照",
            Self::Contract => "合同",
            Self::Authorization => "授权书",
            Self::FoodLicense => "食品经营许可证",
            Self::LegalPersonId => "法人身份证",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Certificate => "certificate",
            Self::Contract => "contract",
            Self::Authorization => "authorization",
            Self::FoodLicense => "food_license",
            Self::LegalPersonId => "legal_person_id",
        }
    }
}

/// 资质状态（§6.2：有效、失效、停用）。
///
/// 状态机：`Active` → `Expired`（到期）、`Active` ⇄ `Disabled`（人工启停）、
/// `Expired` → `Active`（重新维护/续期）；邻接矩阵对称闭合（§4.6 / §13.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationStatus {
    /// 有效。
    #[default]
    Active,
    /// 失效（到期）。
    Expired,
    /// 停用。
    Disabled,
}

impl QualificationStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "有效",
            Self::Expired => "失效",
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
            Self::Expired => "expired",
            Self::Disabled => "disabled",
        }
    }

    /// 判断资质当前是否可用于业务（§6.2：启用可销售公司 SKU、采购单和
    /// 供给关系时必须校验适用能力存在有效资质）。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Active)
    }
}

impl DocumentState for QualificationStatus {
    /// 返回合法后继：有效 → 失效/停用；停用 → 有效；失效 → 有效（续期）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Expired, Self::Disabled],
            Self::Disabled => &[Self::Active],
            Self::Expired => &[Self::Active],
        }
    }
}

/// 资质创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierQualificationData {
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 资质类型。
    pub qualification_type: QualificationType,
    /// 证书编号（供应商、类型、编号组合唯一；合同/授权书用对应编号）。
    pub certificate_no: String,
    /// 发证机构。
    pub issuer: Option<String>,
    /// 生效、失效日期。
    pub valid_from: BusinessDate,
    /// 失效日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 资质附件（受控下载，记录访问审计）。
    pub attachment_id: Option<FileAssetId>,
    /// 状态（有效、失效、停用）。
    pub status: QualificationStatus,
}

/// 资质更新数据。
///
/// `supplier_id`、`qualification_type` 与 `certificate_no` 是稳定身份
/// （组合唯一，§6.2），不允许在通用更新中修改。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierQualificationUpdate {
    /// 发证机构更新意图。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub issuer: FieldUpdate<String>,
    /// 附件更新意图。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub attachment_id: FieldUpdate<FileAssetId>,
    /// 生效日期；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<BusinessDate>,
    /// 失效日期更新意图（`Set` 时校验晚于 `valid_from`）。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub valid_to: FieldUpdate<BusinessDate>,
    /// 状态；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<QualificationStatus>,
}

/// 供应商资质实体（稳定基础资料，§6.2）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SupplierQualification {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<QualificationStatus>,
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
}

impl PartialEq for SupplierQualification {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.supplier_id == other.supplier_id
            && self.qualification_type == other.qualification_type
            && self.certificate_no == other.certificate_no
            && self.issuer == other.issuer
            && self.valid_from == other.valid_from
            && self.valid_to == other.valid_to
            && self.attachment_id == other.attachment_id
    }
}

impl Eq for SupplierQualification {}

impl SupplierQualification {
    /// 创建供应商资质。
    ///
    /// 完成证书编号必填校验与发证机构的规范化（去首尾空白、长度上限）；
    /// 强制 `valid_to` 晚于 `valid_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierQualificationId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的资质实体。
    ///
    /// # 错误
    /// 当证书编号为空/超长、发证机构超长或生效区间倒挂时返回错误。
    pub fn new(
        id: SupplierQualificationId,
        data: SupplierQualificationData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
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
            stable: StableBase::new(data.status, created_by),
            supplier_id: data.supplier_id,
            qualification_type: data.qualification_type,
            certificate_no,
            issuer,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            attachment_id: data.attachment_id,
        })
    }

    /// 更新供应商资质。
    ///
    /// `supplier_id`、`qualification_type` 与 `certificate_no` 是稳定身份，
    /// 不允许在通用更新中修改；状态迁移按固定状态机校验（§13.3）。
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
    pub fn update(
        &mut self,
        update: SupplierQualificationUpdate,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        self.apply_issuer(update.issuer)?;
        self.apply_attachment(update.attachment_id);
        self.apply_valid_window(update.valid_from, update.valid_to)?;
        self.apply_status(update.status)?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 标记资质到期失效。
    ///
    /// 由 P3 按业务日期对照 `valid_to` 触发；状态机仅允许
    /// `Active → Expired`（§6.2：有效、失效、停用）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不是 `Active` 时返回
    /// [`crate::errors::Error::InvalidStateTransition`]。
    pub fn mark_expired(&mut self) -> Result<()> {
        ensure_transition(self.stable.status, QualificationStatus::Expired)?;
        self.stable.status = QualificationStatus::Expired;
        Ok(())
    }

    /// 判断资质当前是否有效（§6.2 业务校验用）。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_valid(&self) -> bool {
        self.stable.status().is_valid()
    }

    /// 应用发证机构更新。
    ///
    /// # 参数
    /// * `update` - 机构更新意图
    ///
    /// # 错误
    /// 当机构名称超长时返回错误。
    fn apply_issuer(&mut self, update: FieldUpdate<String>) -> Result<()> {
        match update {
            FieldUpdate::Unchanged => {}
            FieldUpdate::Clear => self.issuer = None,
            FieldUpdate::Set(value) => {
                self.issuer = normalize_optional_text(Some(value), "发证机构", ISSUER_MAX_LEN)?
            }
        }
        Ok(())
    }

    /// 应用附件更新。
    ///
    /// # 参数
    /// * `update` - 附件更新意图
    fn apply_attachment(&mut self, update: FieldUpdate<FileAssetId>) {
        update.apply_to(&mut self.attachment_id);
    }

    /// 整体应用生效区间更新。
    ///
    /// # 参数
    /// * `valid_from` - 可选的新生效日期
    /// * `valid_to` - 失效日期更新意图
    ///
    /// # 错误
    /// 当失效日期不晚于生效日期时返回错误。
    fn apply_valid_window(
        &mut self,
        valid_from: Option<BusinessDate>,
        valid_to: FieldUpdate<BusinessDate>,
    ) -> Result<()> {
        let next_valid_from = valid_from.unwrap_or(self.valid_from);
        let next_valid_to = match valid_to {
            FieldUpdate::Unchanged => self.valid_to,
            FieldUpdate::Clear => None,
            FieldUpdate::Set(value) => Some(value),
        };
        ensure_window_valid(next_valid_from, next_valid_to)?;
        self.valid_from = next_valid_from;
        self.valid_to = next_valid_to;
        Ok(())
    }

    /// 应用状态更新。
    ///
    /// # 参数
    /// * `status` - 可选目标状态
    ///
    /// # 错误
    /// 目标状态不在固定状态机后继中时返回错误。
    fn apply_status(&mut self, status: Option<QualificationStatus>) -> Result<()> {
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
        QualificationStatus, QualificationType, SupplierQualification, SupplierQualificationData,
        SupplierQualificationUpdate,
    };
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::common::time::BusinessDate;
    use crate::field_update::FieldUpdate;
    use crate::ids::{FileAssetId, SupplierAccountId, SupplierQualificationId};

    fn qualification_data() -> SupplierQualificationData {
        SupplierQualificationData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            qualification_type: QualificationType::Contract,
            certificate_no: " HT-2026-001 ".to_string(),
            issuer: Some(" 示例发证机构 ".to_string()),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            attachment_id: Some(FileAssetId::new("file-1")),
            status: QualificationStatus::Active,
        }
    }

    /// happy path：编号去空白，类型/附件/状态落库。
    #[test]
    fn new_trims_and_normalizes() {
        let qualification = SupplierQualification::new(
            SupplierQualificationId::new("qual-1"),
            qualification_data(),
            "admin-1",
        )
        .unwrap();
        assert_eq!(qualification.certificate_no, "HT-2026-001");
        assert_eq!(qualification.issuer.as_deref(), Some("示例发证机构"));
        assert_eq!(qualification.attachment_id, Some(FileAssetId::new("file-1")));
        assert_eq!(qualification.qualification_type, QualificationType::Contract);
        assert!(qualification.is_valid());
    }

    /// 失败路径：编号为空/超长、机构超长、区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank = SupplierQualificationData {
            certificate_no: "   ".to_string(),
            ..qualification_data()
        };
        assert!(SupplierQualification::new(SupplierQualificationId::new("q"), blank, "admin-1").is_err());

        let overlong = SupplierQualificationData {
            certificate_no: "x".repeat(129),
            ..qualification_data()
        };
        assert!(SupplierQualification::new(SupplierQualificationId::new("q"), overlong, "admin-1").is_err());

        let reversed = SupplierQualificationData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..qualification_data()
        };
        assert!(SupplierQualification::new(SupplierQualificationId::new("q"), reversed, "admin-1").is_err());
    }

    /// 状态机：邻接矩阵闭包完整（有效 ⇄ 失效、有效 ⇄ 停用、失效 → 有效
    /// 续期边），并对不存在的边做逐边定向断言。
    #[test]
    fn status_transitions_follow_fixed_matrix() {
        assert_adjacency_closed(&[
            QualificationStatus::Active,
            QualificationStatus::Expired,
            QualificationStatus::Disabled,
        ]);

        let forbidden: [(QualificationStatus, QualificationStatus); 2] = [
            (QualificationStatus::Expired, QualificationStatus::Disabled),
            (QualificationStatus::Disabled, QualificationStatus::Expired),
        ];
        for (from, to) in forbidden {
            assert!(
                ensure_transition(from, to).is_err(),
                "{from:?} → {to:?} 应为非法迁移"
            );
        }
    }

    /// 到期标记：仅 Active → Expired 合法；更新支持续期回到有效。
    #[test]
    fn expiry_and_renewal_transitions() {
        let mut qualification = SupplierQualification::new(
            SupplierQualificationId::new("qual-2"),
            qualification_data(),
            "admin-1",
        )
        .unwrap();

        qualification.mark_expired().unwrap();
        assert!(!qualification.is_valid());
        assert_eq!(qualification.stable.status, QualificationStatus::Expired);

        qualification
            .update(
                SupplierQualificationUpdate {
                    issuer: FieldUpdate::Unchanged,
                    attachment_id: FieldUpdate::Unchanged,
                    valid_from: None,
                    valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2027, 12, 31).unwrap()),
                    status: Some(QualificationStatus::Active),
                },
                "admin-2",
            )
            .unwrap();
        assert!(qualification.is_valid(), "续期（失效 → 有效）合法");
    }

    /// 更新：稳定身份不可修改（不在更新面），机构/附件可更新。
    #[test]
    fn update_keeps_stable_identity() {
        let mut qualification = SupplierQualification::new(
            SupplierQualificationId::new("qual-3"),
            qualification_data(),
            "admin-1",
        )
        .unwrap();
        qualification
            .update(
                SupplierQualificationUpdate {
                    issuer: FieldUpdate::Clear,
                    attachment_id: FieldUpdate::Clear,
                    valid_from: None,
                    valid_to: FieldUpdate::Unchanged,
                    status: Some(QualificationStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(qualification.issuer, None);
        assert_eq!(qualification.attachment_id, None);
        assert!(!qualification.is_valid());
        assert_eq!(qualification.certificate_no, "HT-2026-001");
        assert_eq!(qualification.supplier_id, SupplierAccountId::new("supplier-1"));
    }

    /// 有效期起止按同一窗口更新，并支持明确清空结束日期。
    #[test]
    fn update_replaces_complete_validity_window() {
        let mut qualification = SupplierQualification::new(
            SupplierQualificationId::new("qual-window"),
            qualification_data(),
            "admin-1",
        )
        .unwrap();
        let next_from = BusinessDate::from_ymd(2026, 2, 1).unwrap();
        qualification
            .update(
                SupplierQualificationUpdate {
                    valid_from: Some(next_from),
                    valid_to: FieldUpdate::Clear,
                    ..SupplierQualificationUpdate::default()
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(qualification.valid_from, next_from);
        assert_eq!(qualification.valid_to, None);
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let qualification = SupplierQualification::new(
            SupplierQualificationId::new("qual-4"),
            qualification_data(),
            "admin-1",
        )
        .unwrap();
        let roundtrip: SupplierQualification =
            bson::deserialize_from_document(bson::serialize_to_document(&qualification).unwrap()).unwrap();
        assert_eq!(roundtrip, qualification);
    }
}
