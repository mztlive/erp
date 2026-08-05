//! `sales_return_case` 销售退货/拒收处理单（数据模型 §6.11）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{CustomerAcceptanceId, SalesOrderId, SalesReturnCaseId};
use crate::validation::normalize_required_text;

/// 退货处理号最大长度。
const RETURN_NO_MAX_LEN: usize = 64;
/// 原因最大长度。
const REASON_MAX_LEN: usize = 512;

/// 退货/拒收处理类型（数据模型 §6.11：退货、拒收、短少、服务不通过）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseType {
    /// 退货。
    Return,
    /// 拒收。
    Reject,
    /// 短少。
    Shortage,
    /// 服务不通过。
    ServiceFailed,
}

impl CaseType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Return => "退货",
            Self::Reject => "拒收",
            Self::Shortage => "短少",
            Self::ServiceFailed => "服务不通过",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Return => "return",
            Self::Reject => "reject",
            Self::Shortage => "shortage",
            Self::ServiceFailed => "service_failed",
        }
    }
}

/// 退货路线（数据模型 §6.11：退公司仓、直退供应商、不发生实物退回）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnRoute {
    /// 退公司仓。
    CompanyWarehouse,
    /// 直退供应商。
    DirectToSupplier,
    /// 不发生实物退回。
    NoPhysicalReturn,
}

impl ReturnRoute {
    /// 返回路线的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::CompanyWarehouse => "退公司仓",
            Self::DirectToSupplier => "直退供应商",
            Self::NoPhysicalReturn => "不发生实物退回",
        }
    }

    /// 返回路线的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CompanyWarehouse => "company_warehouse",
            Self::DirectToSupplier => "direct_to_supplier",
            Self::NoPhysicalReturn => "no_physical_return",
        }
    }
}

/// 处理单状态（数据模型 §6.11：草稿、待仓储验收、待采购处理、待财务处理、
/// 处理中、已完成、作废；第 7 章未定义其状态机，固定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SalesReturnCaseStatus {
    /// 草稿。
    #[default]
    Draft,
    /// 待仓储验收。
    PendingWarehouseAcceptance,
    /// 待采购处理。
    PendingProcurement,
    /// 待财务处理。
    PendingFinance,
    /// 处理中。
    Processing,
    /// 已完成。
    Completed,
    /// 作废。
    Voided,
}

impl SalesReturnCaseStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingWarehouseAcceptance => "待仓储验收",
            Self::PendingProcurement => "待采购处理",
            Self::PendingFinance => "待财务处理",
            Self::Processing => "处理中",
            Self::Completed => "已完成",
            Self::Voided => "作废",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::PendingWarehouseAcceptance => "pending_warehouse_acceptance",
            Self::PendingProcurement => "pending_procurement",
            Self::PendingFinance => "pending_finance",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Voided => "voided",
        }
    }

    /// 判断是否处于终态。
    ///
    /// # 返回
    /// 状态为 `Completed` 或 `Voided` 时返回 `true`。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Voided)
    }
}

/// 销售退货/拒收处理单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesReturnCaseData {
    /// 销售退货/拒收处理号（唯一）。
    pub return_no: String,
    /// 原销售单。
    pub sales_order_id: SalesOrderId,
    /// 验收依据（拒收等场景存在）。
    pub acceptance_id: Option<CustomerAcceptanceId>,
    /// 处理类型。
    pub case_type: CaseType,
    /// 原因。
    pub reason: String,
    /// 发现时间。
    pub discovered_at: Instant,
    /// 退货路线。
    pub return_route: ReturnRoute,
}

/// 销售退货/拒收处理单更新数据（终态不可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SalesReturnCaseUpdate {
    /// 原因；`None` 表示不修改。
    pub reason: Option<String>,
    /// 发现时间；`None` 表示不修改。
    pub discovered_at: Option<Instant>,
    /// 退货路线；`None` 表示不修改。
    pub return_route: Option<ReturnRoute>,
    /// 状态；`None` 表示不修改。
    pub status: Option<SalesReturnCaseStatus>,
}

/// 销售退货/拒收处理单实体（处理单主表类，数据模型 §6.11）。
///
/// `return_no` 唯一；累计有效退回数量不得超过已履约数量、处理完成必须校验适用
/// 的库存/采购/资金/发票子任务均已结束是跨实体约束，由 P3 事务校验（§8.3）。
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SalesReturnCase {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<SalesReturnCaseStatus>,
    /// 退货处理号。
    pub return_no: String,
    /// 原销售单。
    pub sales_order_id: SalesOrderId,
    /// 验收依据。
    pub acceptance_id: Option<CustomerAcceptanceId>,
    /// 处理类型。
    pub case_type: CaseType,
    /// 原因。
    pub reason: String,
    /// 发现时间。
    pub discovered_at: Instant,
    /// 退货路线。
    pub return_route: ReturnRoute,
}

impl PartialEq for SalesReturnCase {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.return_no == other.return_no
            && self.sales_order_id == other.sales_order_id
            && self.acceptance_id == other.acceptance_id
            && self.case_type == other.case_type
            && self.reason == other.reason
            && self.discovered_at == other.discovered_at
            && self.return_route == other.return_route
    }
}

impl Eq for SalesReturnCase {}

impl SalesReturnCase {
    /// 创建销售退货/拒收处理单。
    ///
    /// 完成退货处理号与原因的 trim/非空/长度校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesReturnCaseId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的处理单实体（状态为草稿）。
    ///
    /// # 错误
    /// 当退货处理号或原因为空/超长时返回错误。
    pub fn new(
        id: SalesReturnCaseId,
        data: SalesReturnCaseData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let return_no = normalize_required_text(
            data.return_no,
            "退货处理号不能为空",
            RETURN_NO_MAX_LEN,
            "退货处理号过长",
        )?;
        let reason = normalize_required_text(data.reason, "原因不能为空", REASON_MAX_LEN, "原因过长")?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(SalesReturnCaseStatus::Draft, created_by),
            return_no,
            sales_order_id: data.sales_order_id,
            acceptance_id: data.acceptance_id,
            case_type: data.case_type,
            reason,
            discovered_at: data.discovered_at,
            return_route: data.return_route,
        })
    }

    /// 更新退货/拒收处理单。
    ///
    /// 复用 `new` 的校验规则；已完成与作废是终态（§6.11），终态不可编辑；
    /// 原销售单与处理类型是固定字段不允许修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态为终态或原因为空/超长时返回错误。
    pub fn update(&mut self, update: SalesReturnCaseUpdate, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status().is_terminal() {
            return Err(Error::from("已完成或作废的处理单不可编辑"));
        }
        if let Some(reason) = update.reason {
            self.reason = normalize_required_text(reason, "原因不能为空", REASON_MAX_LEN, "原因过长")?;
        }
        if let Some(discovered_at) = update.discovered_at {
            self.discovered_at = discovered_at;
        }
        if let Some(return_route) = update.return_route {
            self.return_route = return_route;
        }
        if let Some(status) = update.status {
            self.stable.status = status;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断处理单是否已完成。
    ///
    /// # 返回
    /// 状态为 `Completed` 时返回 `true`。
    pub fn is_completed(&self) -> bool {
        self.stable.status() == SalesReturnCaseStatus::Completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> SalesReturnCaseData {
        SalesReturnCaseData {
            return_no: " SR-2026-001 ".to_string(),
            sales_order_id: SalesOrderId::new("so-1"),
            acceptance_id: Some(CustomerAcceptanceId::new("ac-1")),
            case_type: CaseType::Return,
            reason: " 商品破损 ".to_string(),
            discovered_at: Instant::from_unix_secs(1_700_000_000),
            return_route: ReturnRoute::CompanyWarehouse,
        }
    }

    #[test]
    fn new_trims_text_fields_and_starts_as_draft() {
        let case = SalesReturnCase::new(SalesReturnCaseId::new("src-1"), data(), "admin-1").unwrap();

        assert_eq!(case.return_no, "SR-2026-001");
        assert_eq!(case.reason, "商品破损");
        assert_eq!(case.stable.status(), SalesReturnCaseStatus::Draft);
        assert!(!case.is_completed());
    }

    #[test]
    fn new_rejects_blank_no_and_overlong_reason() {
        let blank_no = SalesReturnCaseData {
            return_no: "   ".to_string(),
            ..data()
        };
        assert!(SalesReturnCase::new(SalesReturnCaseId::new("src-2"), blank_no, "admin").is_err());

        let overlong = SalesReturnCaseData {
            reason: "r".repeat(513),
            ..data()
        };
        assert!(SalesReturnCase::new(SalesReturnCaseId::new("src-3"), overlong, "admin").is_err());
    }

    #[test]
    fn update_applies_draft_changes_and_rejects_terminal() {
        let mut case = SalesReturnCase::new(SalesReturnCaseId::new("src-1"), data(), "admin-1").unwrap();

        case.update(
            SalesReturnCaseUpdate {
                reason: Some(" 客户拒收 ".to_string()),
                return_route: Some(ReturnRoute::DirectToSupplier),
                status: Some(SalesReturnCaseStatus::Processing),
                ..Default::default()
            },
            "admin-2",
        )
        .unwrap();
        assert_eq!(case.reason, "客户拒收");
        assert_eq!(case.stable.status(), SalesReturnCaseStatus::Processing);
        assert_eq!(case.stable.updated_by, "admin-2");
        assert_eq!(case.return_no, "SR-2026-001", "关键字段不改");

        case.update(
            SalesReturnCaseUpdate {
                status: Some(SalesReturnCaseStatus::Completed),
                ..Default::default()
            },
            "admin-2",
        )
        .unwrap();
        assert!(case.is_completed());
        assert!(case
            .update(
                SalesReturnCaseUpdate {
                    reason: Some("新原因".to_string()),
                    ..Default::default()
                },
                "admin-3",
            )
            .is_err());
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&CaseType::Shortage).unwrap(),
            "\"shortage\""
        );
        assert_eq!(
            serde_json::to_string(&ReturnRoute::NoPhysicalReturn).unwrap(),
            "\"no_physical_return\""
        );
        assert_eq!(
            serde_json::to_string(&SalesReturnCaseStatus::PendingFinance).unwrap(),
            "\"pending_finance\""
        );
        assert_eq!(CaseType::ServiceFailed.label(), "服务不通过");
        assert_eq!(ReturnRoute::CompanyWarehouse.label(), "退公司仓");
        assert_eq!(SalesReturnCaseStatus::Voided.label(), "作废");
        assert!(SalesReturnCaseStatus::Completed.is_terminal());
        assert!(!SalesReturnCaseStatus::Draft.is_terminal());
    }
}
