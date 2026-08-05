//! `purchase_return_order` 采购退货单（数据模型 §6.11）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::errors::{Error, Result};
use crate::ids::{PurchaseOrderId, PurchaseReturnOrderId, SalesReturnCaseId};
use crate::validation::normalize_required_text;

/// 采购退货单号最大长度。
const RETURN_NO_MAX_LEN: usize = 64;

/// 退货模式（数据模型 §6.11：公司仓退供应商或客户直退供应商）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnMode {
    /// 公司仓退供应商。
    CompanyWarehouseToSupplier,
    /// 客户直退供应商。
    DirectToSupplier,
}

impl ReturnMode {
    /// 返回模式的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::CompanyWarehouseToSupplier => "公司仓退供应商",
            Self::DirectToSupplier => "客户直退供应商",
        }
    }

    /// 返回模式的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CompanyWarehouseToSupplier => "company_warehouse_to_supplier",
            Self::DirectToSupplier => "direct_to_supplier",
        }
    }
}

/// 采购退货单状态（数据模型 §6.11：草稿、待执行、已退货、已完成、作废；
/// 第 7 章未定义其状态机，固定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseReturnStatus {
    /// 草稿。
    #[default]
    Draft,
    /// 待执行。
    PendingExecution,
    /// 已退货。
    Returned,
    /// 已完成。
    Completed,
    /// 作废。
    Voided,
}

impl PurchaseReturnStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingExecution => "待执行",
            Self::Returned => "已退货",
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
            Self::PendingExecution => "pending_execution",
            Self::Returned => "returned",
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

/// 采购退货单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseReturnOrderData {
    /// 采购退货单号（唯一）。
    pub purchase_return_no: String,
    /// 原采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 客户侧依据（销售退货/拒收处理单，可空）。
    pub sales_return_case_id: Option<SalesReturnCaseId>,
    /// 退货模式。
    pub return_mode: ReturnMode,
}

/// 采购退货单更新数据（终态不可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PurchaseReturnOrderUpdate {
    /// 退货模式；`None` 表示不修改。
    pub return_mode: Option<ReturnMode>,
    /// 状态；`None` 表示不修改。
    pub status: Option<PurchaseReturnStatus>,
}

/// 采购退货单实体（处理单主表类，数据模型 §6.11）。
///
/// `purchase_return_no` 唯一；公司仓退货在同一事务形成库存减少和适用预占释放、
/// 客户直退供应商不写自有库存是跨实体约束，由 P3 事务校验（§8.2）；供应商
/// 退款、进项红票和应付冲减另行追加。`StableBase` 是 P0 冻结基元且未派生
/// `PartialEq`，因此本实体手工实现 `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct PurchaseReturnOrder {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<PurchaseReturnStatus>,
    /// 采购退货单号。
    pub purchase_return_no: String,
    /// 原采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 客户侧依据。
    pub sales_return_case_id: Option<SalesReturnCaseId>,
    /// 退货模式。
    pub return_mode: ReturnMode,
}

impl PartialEq for PurchaseReturnOrder {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.purchase_return_no == other.purchase_return_no
            && self.purchase_order_id == other.purchase_order_id
            && self.sales_return_case_id == other.sales_return_case_id
            && self.return_mode == other.return_mode
    }
}

impl Eq for PurchaseReturnOrder {}

impl PurchaseReturnOrder {
    /// 创建采购退货单。
    ///
    /// 完成采购退货单号的 trim/非空/长度校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseReturnOrderId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的退货单实体（状态为草稿）。
    ///
    /// # 错误
    /// 当采购退货单号为空/超长时返回错误。
    pub fn new(
        id: PurchaseReturnOrderId,
        data: PurchaseReturnOrderData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let purchase_return_no = normalize_required_text(
            data.purchase_return_no,
            "采购退货单号不能为空",
            RETURN_NO_MAX_LEN,
            "采购退货单号过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(PurchaseReturnStatus::Draft, created_by),
            purchase_return_no,
            purchase_order_id: data.purchase_order_id,
            sales_return_case_id: data.sales_return_case_id,
            return_mode: data.return_mode,
        })
    }

    /// 更新采购退货单。
    ///
    /// 已完成与作废是终态（§6.11），终态不可编辑；原采购单与退货单号是固定
    /// 字段不允许修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态为终态时返回错误。
    pub fn update(&mut self, update: PurchaseReturnOrderUpdate, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status().is_terminal() {
            return Err(Error::from("已完成或作废的退货单不可编辑"));
        }
        if let Some(return_mode) = update.return_mode {
            self.return_mode = return_mode;
        }
        if let Some(status) = update.status {
            self.stable.status = status;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断退货单是否已完成。
    ///
    /// # 返回
    /// 状态为 `Completed` 时返回 `true`。
    pub fn is_completed(&self) -> bool {
        self.stable.status() == PurchaseReturnStatus::Completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> PurchaseReturnOrderData {
        PurchaseReturnOrderData {
            purchase_return_no: " PR-2026-001 ".to_string(),
            purchase_order_id: PurchaseOrderId::new("po-1"),
            sales_return_case_id: Some(SalesReturnCaseId::new("src-1")),
            return_mode: ReturnMode::CompanyWarehouseToSupplier,
        }
    }

    #[test]
    fn new_trims_text_fields_and_starts_as_draft() {
        let order = PurchaseReturnOrder::new(PurchaseReturnOrderId::new("pro-1"), data(), "admin-1").unwrap();

        assert_eq!(order.purchase_return_no, "PR-2026-001");
        assert_eq!(order.return_mode, ReturnMode::CompanyWarehouseToSupplier);
        assert_eq!(order.stable.status(), PurchaseReturnStatus::Draft);
        assert!(!order.is_completed());
    }

    #[test]
    fn new_rejects_blank_no() {
        let blank = PurchaseReturnOrderData {
            purchase_return_no: "   ".to_string(),
            ..data()
        };
        assert!(PurchaseReturnOrder::new(PurchaseReturnOrderId::new("pro-2"), blank, "admin").is_err());

        let overlong = PurchaseReturnOrderData {
            purchase_return_no: "x".repeat(65),
            ..data()
        };
        assert!(PurchaseReturnOrder::new(PurchaseReturnOrderId::new("pro-3"), overlong, "admin").is_err());
    }

    #[test]
    fn update_applies_changes_and_rejects_terminal() {
        let mut order =
            PurchaseReturnOrder::new(PurchaseReturnOrderId::new("pro-1"), data(), "admin-1").unwrap();

        order
            .update(
                PurchaseReturnOrderUpdate {
                    return_mode: Some(ReturnMode::DirectToSupplier),
                    status: Some(PurchaseReturnStatus::Returned),
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(order.return_mode, ReturnMode::DirectToSupplier);
        assert_eq!(order.stable.status(), PurchaseReturnStatus::Returned);
        assert_eq!(order.stable.updated_by, "admin-2");
        assert_eq!(order.purchase_return_no, "PR-2026-001", "关键字段不改");

        order
            .update(
                PurchaseReturnOrderUpdate {
                    status: Some(PurchaseReturnStatus::Completed),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();
        assert!(order.is_completed());
        assert!(order
            .update(
                PurchaseReturnOrderUpdate {
                    status: Some(PurchaseReturnStatus::PendingExecution),
                    ..Default::default()
                },
                "admin-3",
            )
            .is_err());
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&ReturnMode::DirectToSupplier).unwrap(),
            "\"direct_to_supplier\""
        );
        assert_eq!(
            serde_json::to_string(&PurchaseReturnStatus::PendingExecution).unwrap(),
            "\"pending_execution\""
        );
        assert_eq!(ReturnMode::CompanyWarehouseToSupplier.label(), "公司仓退供应商");
        assert_eq!(PurchaseReturnStatus::Returned.label(), "已退货");
        assert!(PurchaseReturnStatus::Voided.is_terminal());
        assert!(!PurchaseReturnStatus::Draft.is_terminal());
    }
}
