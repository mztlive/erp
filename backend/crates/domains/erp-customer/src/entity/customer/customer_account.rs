//! `customer_account`：客户角色（数据模型 §6.2，页面：W03、W15）。
//!
//! 一个 `party` 最多一个有效客户角色（跨行约束由 P3 事务校验，§6.2）；
//! 停用角色仍可被历史单据引用（§4.5.3：基础资料以停用表示退出业务）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::Result;
use erp_core::common::stable::StableBase;
use erp_core::common::state::{DocumentState, ensure_transition};
use erp_core::field_update::FieldUpdate;
pub use erp_core::ids::{CustomerAccountId, PartyId};
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

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
    /// 把客户账户状态转换为中文展示名称，供详情、列表或业务提示显示。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "启用",
            Self::Disabled => "停用",
        }
    }

    /// 返回客户账户状态的固定代码，供持久化值和查询条件使用同一口径。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    /// 判断是否处于启用状态。
    ///
    /// 处于 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// 返回停用状态阻断的业务动作稳定代码。
    ///
    /// 阻断口径的权威实现：DTO 的 `customer_status_blockers` 只负责组装
    /// 展示文案，动作集合以此处为准。
    ///
    /// 启用时为空；停用时返回禁止新合同与新销售单的动作代码。
    pub fn blocked_actions(&self) -> &'static [&'static str] {
        match self {
            Self::Active => &[],
            Self::Disabled => &["UPLOAD_CONTRACT_PDF", "CREATE_SALES_ORDER"],
        }
    }
}

impl DocumentState for CustomerAccountStatus {
    // 返回合法后继：启用 ⇄ 停用。
    //
    // `Active` 的后继只有 `Disabled`，`Disabled` 的后继只有 `Active`。切片不含当前状态。
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
/// `PartialEq`/`Eq`（全字段语义相等）。新增字段时必须同步 [`Self::eq`] 与
/// [`equality_probe`]，否则字段覆盖测试失败（erp-customer-010）。
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
    // 比较两条客户账户的全部持久化字段，判断其业务内容和存储状态是否完全一致。
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
    /// 以稳定编号关联现有主体并初始化客户账户，供新建客户关系时使用。
    ///
    /// 完成 customer_no 的必填校验与规范化（去首尾空白、非空、长度
    /// 上限），付款条件引用规范化。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::CustomerAccountId`）
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// 返回新建的客户角色实体。
    ///
    /// 当 `customer_no` 为空或超长，或付款条件引用超长时返回错误。
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

    /// 维护客户默认付款条件和账户状态，保留既有单据引用的编号和主体关系。
    ///
    /// `customer_no` 与 `party_id` 是稳定身份，不允许在通用更新中修改；
    /// 状态迁移按固定状态机校验（§13.3）。
    ///
    /// # 参数
    /// * `updated_by` - 本次更新执行人
    ///
    /// 更新成功返回 `Ok(())`。
    ///
    /// 当付款条件引用非法或状态迁移非法时返回错误。
    pub fn update(&mut self, update: CustomerAccountUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.apply_payment_term(update.default_payment_term_id)?;
        self.apply_status(update.status)?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断客户账户是否启用，供合同和销售单据检查客户能否参与新的业务。
    ///
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    /// 写入前确认客户账户仍是调用方读取的版本，发现并发修改时返回版本冲突。
    ///
    /// # 参数
    /// * `expected` - 客户端期望版本
    ///
    /// 版本一致时返回 `Ok(())`。
    ///
    /// 当前版本与期望版本不一致时返回错误。
    pub fn ensure_version(&self, expected: u64) -> Result<()> {
        super::ensure_entity_version(self.base.version, expected)
    }

    // 在写入客户账户前，把设置、清空或保留付款条件转换成对应的字段更新。
    //
    // # 参数
    // * `update` - 付款条件更新意图
    //
    // 当引用超长时返回错误。
    fn apply_payment_term(&mut self, update: FieldUpdate<String>) -> Result<()> {
        match update {
            FieldUpdate::Unchanged => {},
            FieldUpdate::Clear => self.default_payment_term_id = None,
            FieldUpdate::Set(value) => self.default_payment_term_id = normalize_payment_term_id(Some(value))?,
        }
        Ok(())
    }

    // 核对客户账户状态能否切换，再应用新的启停状态供后续业务检查使用。
    //
    // # 参数
    // * `status` - 可选目标状态
    //
    // 目标状态不在固定状态机后继中时返回错误。
    fn apply_status(&mut self, status: Option<CustomerAccountStatus>) -> Result<()> {
        if let Some(to) = status {
            ensure_transition(self.stable.status, to)?;
            self.stable.status = to;
        }
        Ok(())
    }
}

// 校验并清理默认付款条件标识，将空值归为未配置，供客户建档和更新共用。
//
// 空值规范化为 `None`；非空值去首尾空白并校验长度上限。
// 付款条件是跨域受控码表字典（`default_payment_term_id` 解析），
// 实体层只保存引用不校验码表内容（**地基修订候选**：共享
// `PaymentTermId` newtype 与付款条件值对象下沉到 `common/`）。
//
// # 参数
// * `value` - 原始输入
//
// 返回规范化后的引用或 `None`。
//
// 当引用超长时返回错误。
fn normalize_payment_term_id(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > PAYMENT_TERM_ID_MAX_LEN {
        return Err(erp_core::Error::from("付款条件标识过长，请重新选择付款条件"));
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use erp_core::common::state::assert_adjacency_closed;
    use erp_core::field_update::FieldUpdate;
    use erp_core::ids::{CustomerAccountId, PartyId};

    use super::{CustomerAccount, CustomerAccountData, CustomerAccountStatus, CustomerAccountUpdate};

    fn account_data() -> CustomerAccountData {
        CustomerAccountData {
            party_id: PartyId::new("party-1"),
            customer_no: " C-2026-001 ".to_string(),
            default_payment_term_id: Some(" POSTPAY_NET30 ".to_string()),
            status: CustomerAccountStatus::Active,
        }
    }

    // 验证：编号去空白，付款条件引用规范化。
    #[test]
    fn new_trims_and_normalizes() {
        let account =
            CustomerAccount::new(CustomerAccountId::new("customer-1"), account_data(), "admin-1").unwrap();
        assert_eq!(account.customer_no, "C-2026-001");
        assert_eq!(account.default_payment_term_id.as_deref(), Some("POSTPAY_NET30"));
        assert!(account.is_active());
        assert_eq!(account.stable.created_by, "admin-1");
    }

    // 验证：编号为空/超长、引用超长。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank = CustomerAccountData { customer_no: "   ".to_string(), ..account_data() };
        assert!(CustomerAccount::new(CustomerAccountId::new("c"), blank, "admin-1").is_err());

        let overlong = CustomerAccountData { customer_no: "x".repeat(65), ..account_data() };
        assert!(CustomerAccount::new(CustomerAccountId::new("c"), overlong, "admin-1").is_err());

        let overlong_term =
            CustomerAccountData { default_payment_term_id: Some("t".repeat(65)), ..account_data() };
        assert!(CustomerAccount::new(CustomerAccountId::new("c"), overlong_term, "admin-1").is_err());
    }

    // 状态机：邻接矩阵闭包完整，合法迁移通过。
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

    // 更新：编号与主体不可修改（不在更新面），付款条件可设置。
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

    // 版本校验接受当前版本并拒绝过期版本。
    #[test]
    fn version_check_rejects_stale_expected_version() {
        let account =
            CustomerAccount::new(CustomerAccountId::new("customer-version"), account_data(), "admin-1")
                .unwrap();
        assert!(account.ensure_version(1).is_ok());
        assert!(account.ensure_version(2).is_err());
    }

    // 阻断口径：启用无阻断，停用阻断新合同与新销售单。
    #[test]
    fn blocked_actions_follow_status() {
        assert!(CustomerAccountStatus::Active.blocked_actions().is_empty());
        assert_eq!(
            CustomerAccountStatus::Disabled.blocked_actions(),
            &["UPLOAD_CONTRACT_PDF", "CREATE_SALES_ORDER"]
        );
    }

    // 手工 `PartialEq` 的字段覆盖约束（erp-customer-010）。
    //
    // 经 `serde_json::Value` 枚举全部持久化字段；新增字段未同步 `eq`
    // 实现时，本测试因对象差异未被比对而失败。
    // `base`/`stable` 以整体比较覆盖其全部子字段。
    #[test]
    fn manual_partial_eq_covers_all_persisted_fields() {
        let left =
            CustomerAccount::new(CustomerAccountId::new("customer-eq"), account_data(), "admin-1").unwrap();
        let value = serde_json::to_value(&left).unwrap();
        let object = value.as_object().unwrap();
        let mut covered = std::collections::BTreeSet::new();
        for key in object.keys() {
            covered.insert(key.as_str());
        }

        for key in [
            "id",
            "version",
            "created_at",
            "updated_at",
            "deleted_at",
            "status",
            "current_revision_id",
            "created_by",
            "updated_by",
            "party_id",
            "customer_no",
            "default_payment_term_id",
        ] {
            assert!(covered.contains(key), "持久化字段未覆盖：{key}");
        }
        assert_eq!(covered.len(), 12, "新增持久化字段必须同步 eq 实现与本清单：{covered:?}");

        let mut right = left.clone();
        assert_eq!(left, right);
        right.customer_no = "C-2026-002".to_string();
        assert_ne!(left, right);
    }
}
