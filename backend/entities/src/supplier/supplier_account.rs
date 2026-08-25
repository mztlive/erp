//! `supplier_account`：供应商角色（数据模型 §6.2，页面：W14）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::errors::Result;
use crate::field_update::FieldUpdate;
use crate::validation::normalize_required_text;

pub use crate::ids::{PartyId, SupplierAccountId, SupplierCommercialProfileRevisionId};

/// 供应商编号最大长度。
const SUPPLIER_NO_MAX_LEN: usize = 64;
/// 结算条件引用最大长度。
const PAYMENT_TERM_ID_MAX_LEN: usize = 64;

/// 供应商角色启停状态（§6.2：启用/停用；对称状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierAccountStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

impl SupplierAccountStatus {
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

impl DocumentState for SupplierAccountStatus {
    /// 返回合法后继：启用 ⇄ 停用。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Disabled],
            Self::Disabled => &[Self::Active],
        }
    }
}

/// 供应商资料修订前的纯状态校验结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplierProfileUpdateViolation {
    /// 客户端携带的版本已过期。
    VersionMismatch,
    /// 供应商角色已停用。
    SupplierDisabled,
}

/// 供应商角色创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierAccountData {
    /// 共用企业主体 ID。
    pub party_id: PartyId,
    /// 供应商编号（全局唯一，创建后不可修改）。
    pub supplier_no: String,
    /// 默认供应商结算条件引用（受控码表字典）。
    pub default_payment_term_id: Option<String>,
    /// 供应商当前商务结算版本 ID；客户角色不适用。
    pub current_commercial_profile_revision_id: Option<SupplierCommercialProfileRevisionId>,
    /// 启停状态。
    pub status: SupplierAccountStatus,
}

/// 供应商角色更新数据。
///
/// `party_id` 与 `supplier_no` 是稳定身份，不允许在通用更新中修改。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierAccountUpdate {
    /// 默认结算条件更新意图（`Unchanged` 保留、`Clear` 清除、`Set` 设置）。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub default_payment_term_id: FieldUpdate<String>,
    /// 当前商务结算版本更新意图（P3 形成新商务版本时推进）。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub current_commercial_profile_revision_id: FieldUpdate<SupplierCommercialProfileRevisionId>,
    /// 启停状态；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<SupplierAccountStatus>,
}

/// 供应商角色实体（稳定基础资料，§6.2）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SupplierAccount {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<SupplierAccountStatus>,
    /// 共用企业主体 ID（创建后不可修改）。
    pub party_id: PartyId,
    /// 供应商编号（§4.1：编号一经形成正式事实不得复用）。
    pub supplier_no: String,
    /// 默认供应商结算条件引用。
    pub default_payment_term_id: Option<String>,
    /// 供应商当前商务结算版本 ID（§6.2：供应商角色专用）。
    pub current_commercial_profile_revision_id: Option<SupplierCommercialProfileRevisionId>,
}

impl PartialEq for SupplierAccount {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.party_id == other.party_id
            && self.supplier_no == other.supplier_no
            && self.default_payment_term_id == other.default_payment_term_id
            && self.current_commercial_profile_revision_id == other.current_commercial_profile_revision_id
    }
}

impl Eq for SupplierAccount {}

impl SupplierAccount {
    /// 创建供应商角色。
    ///
    /// 完成 supplier_no 的必填校验与规范化（去首尾空白、非空、长度
    /// 上限），结算条件引用规范化。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierAccountId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的供应商角色实体。
    ///
    /// # 错误
    /// 当 supplier_no 为空/超长时返回错误。
    pub fn new(
        id: SupplierAccountId,
        data: SupplierAccountData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let supplier_no = normalize_required_text(
            data.supplier_no,
            "供应商编号不能为空",
            SUPPLIER_NO_MAX_LEN,
            "供应商编号过长",
        )?;
        let default_payment_term_id = normalize_payment_term_id(data.default_payment_term_id)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            party_id: data.party_id,
            supplier_no,
            default_payment_term_id,
            current_commercial_profile_revision_id: data.current_commercial_profile_revision_id,
        })
    }

    /// 更新供应商角色。
    ///
    /// `supplier_no` 与 `party_id` 是稳定身份，不允许在通用更新中修改；
    /// 状态迁移按固定状态机校验（§13.3）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当结算条件引用非法或状态迁移非法时返回错误。
    pub fn update(&mut self, update: SupplierAccountUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.apply_payment_term(update.default_payment_term_id)?;
        self.apply_profile_revision(update.current_commercial_profile_revision_id);
        self.apply_status(update.status)?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断供应商角色是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    /// 判断当前供应商是否允许从指定乐观锁版本修订资料。
    ///
    /// # 参数
    /// * `expected_version` - 客户端读取资料时看到的供应商版本
    ///
    /// # 返回
    /// 版本一致且供应商启用时返回 `None`；否则返回稳定违反原因。
    pub fn profile_update_violation(&self, expected_version: u64) -> Option<SupplierProfileUpdateViolation> {
        if self.base.version != expected_version {
            return Some(SupplierProfileUpdateViolation::VersionMismatch);
        }
        if !self.is_active() {
            return Some(SupplierProfileUpdateViolation::SupplierDisabled);
        }
        None
    }

    /// 应用默认结算条件更新。
    ///
    /// # 参数
    /// * `update` - 结算条件更新意图
    ///
    /// # 错误
    /// 当引用超长时返回错误。
    fn apply_payment_term(&mut self, update: FieldUpdate<String>) -> Result<()> {
        match update {
            FieldUpdate::Unchanged => {}
            FieldUpdate::Clear => self.default_payment_term_id = None,
            FieldUpdate::Set(value) => self.default_payment_term_id = normalize_payment_term_id(Some(value))?,
        }
        Ok(())
    }

    /// 应用当前商务结算版本更新。
    ///
    /// # 参数
    /// * `update` - 版本引用更新意图
    fn apply_profile_revision(&mut self, update: FieldUpdate<SupplierCommercialProfileRevisionId>) {
        update.apply_to(&mut self.current_commercial_profile_revision_id);
    }

    /// 应用状态更新。
    ///
    /// # 参数
    /// * `status` - 可选目标状态
    ///
    /// # 错误
    /// 目标状态不在固定状态机后继中时返回错误。
    fn apply_status(&mut self, status: Option<SupplierAccountStatus>) -> Result<()> {
        if let Some(to) = status {
            ensure_transition(self.stable.status, to)?;
            self.stable.status = to;
        }
        Ok(())
    }
}

/// 规范化结算条件引用。
///
/// 空值规范化为 `None`；非空值去首尾空白并校验长度上限。
/// 结算条件是跨域受控码表字典（`default_payment_term_id` 解析），
/// 实体层只保存引用不校验码表内容（**地基修订候选**：共享
/// `PaymentTermId` newtype 与付款条件值对象下沉到 `common/`）。
///
/// # 参数
/// * `value` - 原始输入
///
/// # 返回
/// 返回规范化后的引用或 `None`。
///
/// # 错误
/// 当引用超长时返回错误。
fn normalize_payment_term_id(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > PAYMENT_TERM_ID_MAX_LEN {
        return Err(crate::errors::Error::from("结算条件引用过长"));
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::{
        SupplierAccount, SupplierAccountData, SupplierAccountStatus, SupplierAccountUpdate,
        SupplierProfileUpdateViolation,
    };
    use crate::common::state::assert_adjacency_closed;
    use crate::field_update::FieldUpdate;
    use crate::ids::{PartyId, SupplierAccountId, SupplierCommercialProfileRevisionId};

    fn account_data() -> SupplierAccountData {
        SupplierAccountData {
            party_id: PartyId::new("party-1"),
            supplier_no: " S-2026-001 ".to_string(),
            default_payment_term_id: Some(" PREPAY_30 ".to_string()),
            current_commercial_profile_revision_id: None,
            status: SupplierAccountStatus::Active,
        }
    }

    /// happy path：编号去空白，结算条件引用规范化，商务版本引用保留。
    #[test]
    fn new_trims_and_normalizes() {
        let account =
            SupplierAccount::new(SupplierAccountId::new("supplier-1"), account_data(), "admin-1").unwrap();
        assert_eq!(account.supplier_no, "S-2026-001");
        assert_eq!(account.default_payment_term_id.as_deref(), Some("PREPAY_30"));
        assert!(account.current_commercial_profile_revision_id.is_none());
        assert!(account.is_active());
        assert_eq!(account.stable.created_by, "admin-1");
    }

    /// 失败路径：编号为空/超长、引用超长。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank = SupplierAccountData {
            supplier_no: "   ".to_string(),
            ..account_data()
        };
        assert!(SupplierAccount::new(SupplierAccountId::new("s"), blank, "admin-1").is_err());

        let overlong = SupplierAccountData {
            supplier_no: "x".repeat(65),
            ..account_data()
        };
        assert!(SupplierAccount::new(SupplierAccountId::new("s"), overlong, "admin-1").is_err());

        let overlong_term = SupplierAccountData {
            default_payment_term_id: Some("t".repeat(65)),
            ..account_data()
        };
        assert!(SupplierAccount::new(SupplierAccountId::new("s"), overlong_term, "admin-1").is_err());
    }

    /// 状态机：邻接矩阵闭包完整，合法迁移通过。
    #[test]
    fn status_transitions_follow_fixed_matrix() {
        assert_adjacency_closed(&[SupplierAccountStatus::Active, SupplierAccountStatus::Disabled]);

        let mut account =
            SupplierAccount::new(SupplierAccountId::new("supplier-2"), account_data(), "admin-1").unwrap();
        account
            .update(
                SupplierAccountUpdate {
                    default_payment_term_id: FieldUpdate::Unchanged,
                    current_commercial_profile_revision_id: FieldUpdate::Set(
                        SupplierCommercialProfileRevisionId::new("profile-rev-1"),
                    ),
                    status: Some(SupplierAccountStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        assert!(!account.is_active());
        assert_eq!(
            account
                .current_commercial_profile_revision_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("profile-rev-1")
        );
        assert_eq!(account.stable.updated_by, "admin-2");
    }

    /// 更新：编号与主体不可修改（不在更新面），商务版本可清除。
    #[test]
    fn update_keeps_stable_identity() {
        let mut account = SupplierAccount::new(
            SupplierAccountId::new("supplier-3"),
            SupplierAccountData {
                current_commercial_profile_revision_id: Some(SupplierCommercialProfileRevisionId::new(
                    "profile-rev-1",
                )),
                ..account_data()
            },
            "admin-1",
        )
        .unwrap();
        account
            .update(
                SupplierAccountUpdate {
                    default_payment_term_id: FieldUpdate::Clear,
                    current_commercial_profile_revision_id: FieldUpdate::Unchanged,
                    status: None,
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(account.default_payment_term_id, None);
        assert_eq!(account.supplier_no, "S-2026-001");
        assert_eq!(account.party_id, PartyId::new("party-1"));
        assert!(
            account.current_commercial_profile_revision_id.is_some(),
            "Unchanged 保留原值"
        );
    }

    /// 资料修订先校验版本，再校验供应商启停状态。
    #[test]
    fn profile_update_violation_preserves_gate_order() {
        let mut account =
            SupplierAccount::new(SupplierAccountId::new("supplier-4"), account_data(), "admin-1").unwrap();
        assert_eq!(account.profile_update_violation(1), None);
        assert_eq!(
            account.profile_update_violation(2),
            Some(SupplierProfileUpdateViolation::VersionMismatch)
        );
        account.stable.status = SupplierAccountStatus::Disabled;
        assert_eq!(
            account.profile_update_violation(1),
            Some(SupplierProfileUpdateViolation::SupplierDisabled)
        );
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let account =
            SupplierAccount::new(SupplierAccountId::new("supplier-4"), account_data(), "admin-1").unwrap();
        let roundtrip: SupplierAccount =
            bson::deserialize_from_document(bson::serialize_to_document(&account).unwrap()).unwrap();
        assert_eq!(roundtrip, account);
    }
}
