//! `customer_account`：客户角色（数据模型 §6.2，页面：W03、W15）。
//!
//! 一个 `party` 最多一个有效客户角色（跨行约束由 P3 事务校验，§6.2）；
//! 停用角色仍可被历史单据引用（§4.5.3：基础资料以停用表示退出业务）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::state::{ensure_transition, DocumentState};
use crate::errors::Result;
use crate::field_update::FieldUpdate;
use crate::validation::normalize_required_text;

pub use crate::ids::{CustomerAccountId, PartyId};

/// 客户编号最大长度。
const CUSTOMER_NO_MAX_LEN: usize = 64;
/// 付款条件引用最大长度。
const PAYMENT_TERM_ID_MAX_LEN: usize = 64;

/// 客户角色启停状态（§6.2：启用/停用；对称状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomerAccountStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

impl CustomerAccountStatus {
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

impl DocumentState for CustomerAccountStatus {
    /// 返回合法后继：启用 ⇄ 停用。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Disabled],
            Self::Disabled => &[Self::Active],
        }
    }
}

/// 客户角色创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAccountData {
    /// 共用企业主体 ID。
    pub party_id: PartyId,
    /// 客户编号（全局唯一，创建后不可修改）。
    pub customer_no: String,
    /// 默认客户付款条件引用（受控码表字典；`default_payment_term_id` 解析）。
    pub default_payment_term_id: Option<String>,
    /// 启停状态。
    pub status: CustomerAccountStatus,
}

/// 客户角色更新数据。
///
/// `party_id` 与 `customer_no` 是稳定身份，不允许在通用更新中修改。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAccountUpdate {
    /// 默认付款条件更新意图（`Unchanged` 保留、`Clear` 清除、`Set` 设置）。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub default_payment_term_id: FieldUpdate<String>,
    /// 启停状态；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<CustomerAccountStatus>,
}

/// 客户角色实体（稳定基础资料，§6.2）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct CustomerAccount {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<CustomerAccountStatus>,
    /// 共用企业主体 ID（创建后不可修改）。
    pub party_id: PartyId,
    /// 客户编号（§4.1：编号一经形成正式事实不得复用）。
    pub customer_no: String,
    /// 默认客户付款条件引用；仅录单提示，正式销售以合同/销售快照为准（W03）。
    pub default_payment_term_id: Option<String>,
}

impl PartialEq for CustomerAccount {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.party_id == other.party_id
            && self.customer_no == other.customer_no
            && self.default_payment_term_id == other.default_payment_term_id
    }
}

impl Eq for CustomerAccount {}

impl CustomerAccount {
    /// 创建客户角色。
    ///
    /// 完成 customer_no 的必填校验与规范化（去首尾空白、非空、长度
    /// 上限），付款条件引用规范化。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::CustomerAccountId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的客户角色实体。
    ///
    /// # 错误
    /// 当 customer_no 为空/超长时返回错误。
    pub fn new(
        id: CustomerAccountId,
        data: CustomerAccountData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let customer_no = normalize_required_text(
            data.customer_no,
            "客户编号不能为空",
            CUSTOMER_NO_MAX_LEN,
            "客户编号过长",
        )?;
        let default_payment_term_id = normalize_payment_term_id(data.default_payment_term_id)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            party_id: data.party_id,
            customer_no,
            default_payment_term_id,
        })
    }

    /// 更新客户角色。
    ///
    /// `customer_no` 与 `party_id` 是稳定身份，不允许在通用更新中修改；
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
    /// 当付款条件引用非法或状态迁移非法时返回错误。
    pub fn update(&mut self, update: CustomerAccountUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.apply_payment_term(update.default_payment_term_id)?;
        self.apply_status(update.status)?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断客户角色是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    /// 应用默认付款条件更新。
    ///
    /// # 参数
    /// * `update` - 付款条件更新意图
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

    /// 应用状态更新。
    ///
    /// # 参数
    /// * `status` - 可选目标状态
    ///
    /// # 错误
    /// 目标状态不在固定状态机后继中时返回错误。
    fn apply_status(&mut self, status: Option<CustomerAccountStatus>) -> Result<()> {
        if let Some(to) = status {
            ensure_transition(self.stable.status, to)?;
            self.stable.status = to;
        }
        Ok(())
    }
}

/// 规范化付款条件引用。
///
/// 空值规范化为 `None`；非空值去首尾空白并校验长度上限。
/// 付款条件是跨域受控码表字典（`default_payment_term_id` 解析），
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
        return Err(crate::errors::Error::from("付款条件引用过长"));
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::{CustomerAccount, CustomerAccountData, CustomerAccountStatus, CustomerAccountUpdate};
    use crate::common::state::assert_adjacency_closed;
    use crate::field_update::FieldUpdate;
    use crate::ids::{CustomerAccountId, PartyId};

    fn account_data() -> CustomerAccountData {
        CustomerAccountData {
            party_id: PartyId::new("party-1"),
            customer_no: " C-2026-001 ".to_string(),
            default_payment_term_id: Some(" POSTPAY_NET30 ".to_string()),
            status: CustomerAccountStatus::Active,
        }
    }

    /// happy path：编号去空白，付款条件引用规范化。
    #[test]
    fn new_trims_and_normalizes() {
        let account =
            CustomerAccount::new(CustomerAccountId::new("customer-1"), account_data(), "admin-1").unwrap();
        assert_eq!(account.customer_no, "C-2026-001");
        assert_eq!(account.default_payment_term_id.as_deref(), Some("POSTPAY_NET30"));
        assert!(account.is_active());
        assert_eq!(account.stable.created_by, "admin-1");
    }

    /// 失败路径：编号为空/超长、引用超长。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank = CustomerAccountData {
            customer_no: "   ".to_string(),
            ..account_data()
        };
        assert!(CustomerAccount::new(CustomerAccountId::new("c"), blank, "admin-1").is_err());

        let overlong = CustomerAccountData {
            customer_no: "x".repeat(65),
            ..account_data()
        };
        assert!(CustomerAccount::new(CustomerAccountId::new("c"), overlong, "admin-1").is_err());

        let overlong_term = CustomerAccountData {
            default_payment_term_id: Some("t".repeat(65)),
            ..account_data()
        };
        assert!(CustomerAccount::new(CustomerAccountId::new("c"), overlong_term, "admin-1").is_err());
    }

    /// 状态机：邻接矩阵闭包完整，合法迁移通过。
    #[test]
    fn status_transitions_follow_fixed_matrix() {
        assert_adjacency_closed(&[CustomerAccountStatus::Active, CustomerAccountStatus::Disabled]);

        let mut account =
            CustomerAccount::new(CustomerAccountId::new("customer-2"), account_data(), "admin-1").unwrap();
        account
            .update(
                CustomerAccountUpdate {
                    default_payment_term_id: FieldUpdate::Clear,
                    status: Some(CustomerAccountStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        assert!(!account.is_active());
        assert_eq!(account.default_payment_term_id, None);
        assert_eq!(account.stable.updated_by, "admin-2");
    }

    /// 更新：编号与主体不可修改（不在更新面），付款条件可设置。
    #[test]
    fn update_keeps_stable_identity() {
        let mut account =
            CustomerAccount::new(CustomerAccountId::new("customer-3"), account_data(), "admin-1").unwrap();
        account
            .update(
                CustomerAccountUpdate {
                    default_payment_term_id: FieldUpdate::Set("PREPAY_50".to_string()),
                    status: None,
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(account.default_payment_term_id.as_deref(), Some("PREPAY_50"));
        assert_eq!(account.customer_no, "C-2026-001");
        assert_eq!(account.party_id, PartyId::new("party-1"));
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let account =
            CustomerAccount::new(CustomerAccountId::new("customer-4"), account_data(), "admin-1").unwrap();
        let roundtrip: CustomerAccount = bson::from_document(bson::to_document(&account).unwrap()).unwrap();
        assert_eq!(roundtrip, account);
    }
}
