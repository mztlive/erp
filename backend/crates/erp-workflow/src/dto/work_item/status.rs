use serde::{Deserialize, Serialize};

use crate::entity::work_item::WorkItemType;

pub const WORK_ITEM_TYPES: [WorkItemType; 11] = [
    WorkItemType::DocumentApproval,
    WorkItemType::ProcurementOrderCreation,
    WorkItemType::FulfillmentOperation,
    WorkItemType::CustomerAcceptanceRegistration,
    WorkItemType::SupplierPaymentExecution,
    WorkItemType::SalesInvoiceExecution,
    WorkItemType::SupplierSettlementReview,
    WorkItemType::SupplierPortalReview,
    WorkItemType::ImportBusinessConfirmation,
    WorkItemType::IntegrationResultUnknown,
    WorkItemType::BusinessException,
];

/// 责任队列固定范围。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemScope {
    /// 当前用户已负责的开放任务。
    Mine,
    /// 主管授权组织内全部开放任务。
    Managed,
    /// 当前用户参与过的已完成或已关闭任务。
    History,
}

impl WorkItemScope {
    /// 返回稳定范围代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `mine`、`managed` 或 `history`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Managed => "managed",
            Self::History => "history",
        }
    }
}

/// 任务族筛选。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemFamily {
    /// 采购执行。
    Procurement,
    /// 审批与确认。
    Approval,
    /// 财务处理。
    Finance,
    /// 履约处理。
    Fulfillment,
    /// 异常与补偿。
    Exception,
}

impl WorkItemFamily {
    /// 返回该任务族的服务端注册任务类型。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `family_of` 落在本族、且属于 `WORK_ITEM_TYPES` 的任务类型。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn work_item_types(self) -> Vec<WorkItemType> {
        WORK_ITEM_TYPES.into_iter().filter(|work_item_type| family_of(*work_item_type) == self).collect()
    }
}

/// 队列排序。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemSort {
    /// 优先按时限排序；优先级仍由任务行展示。
    PriorityDue,
    /// 到期时间升序。
    DueAsc,
    /// 创建时间倒序。
    CreatedDesc,
}

/// 当前处理状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProcessingState {
    /// 责任和审批步骤允许继续处理。
    Ready,
    /// 审批步骤受阻，普通责任动作必须为空。
    ApprovalBlocked,
    /// 当前负责人已失去账号、执行权限或订单读取资格；保留责任，由授权管理者处置。
    ExecutionBlocked,
}

/// 权限安全的任务阻断摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcessingBlockerView {
    /// 稳定阻断码。
    pub code: String,
    /// 面向用户且不泄露内部细节的说明。
    pub message: String,
}

/// 服务端计算的允许动作。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemAllowedAction {
    /// 查看对象。
    View,
    /// 进入固定强类型处理器。
    Process,
    /// 审批通过。
    Approve,
    /// 审批驳回。
    Reject,
    /// 受控转交。
    Reassign,
    /// 受控关闭无效任务。
    Close,
}

/// 返回任务类型所属的固定任务族。
///
/// # 参数
/// * `work_item_type` - 已登记的任务类型。
///
/// # 返回
/// 按类型穷尽匹配采购、审批、财务、履约或异常族。
///
/// # 错误
/// 不返回错误。
pub fn family_of(work_item_type: WorkItemType) -> WorkItemFamily {
    match work_item_type {
        WorkItemType::ProcurementOrderCreation => WorkItemFamily::Procurement,
        WorkItemType::FulfillmentOperation | WorkItemType::CustomerAcceptanceRegistration => {
            WorkItemFamily::Fulfillment
        },
        WorkItemType::DocumentApproval
        | WorkItemType::SupplierPortalReview
        | WorkItemType::OwnershipMigrationSalesConfirmation => WorkItemFamily::Approval,
        WorkItemType::CardFundsReview
        | WorkItemType::CardFundsDeltaReview
        | WorkItemType::SupplierPaymentExecution
        | WorkItemType::SalesInvoiceExecution
        | WorkItemType::PurchaseOrderReview
        | WorkItemType::SalesChangeFinanceReview
        | WorkItemType::OwnershipMigrationFinanceConfirmation
        | WorkItemType::FinanceCorrectionReview
        | WorkItemType::SupplierSettlementReview => WorkItemFamily::Finance,
        WorkItemType::SalesChangeImpactReview | WorkItemType::InventoryAdjustmentReview => {
            WorkItemFamily::Fulfillment
        },
        WorkItemType::ImportBusinessConfirmation
        | WorkItemType::IntegrationResultUnknown
        | WorkItemType::BusinessException => WorkItemFamily::Exception,
    }
}
