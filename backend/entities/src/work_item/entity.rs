//! `work_item`：审批与独立人工任务的当前责任事实。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::WorkItemId;
use crate::validation::{normalize_optional_text, normalize_required_text};
use crate::{AccountCore, AccountKind};

use bpm::ApprovalNodeExecutionId;

const OBJECT_TYPE_MAX_LEN: usize = 64;
const OBJECT_ID_MAX_LEN: usize = 128;
const RESPONSIBILITY_KEY_MAX_LEN: usize = 128;
const RESPONSIBILITY_SCOPE_MAX_ITEMS: usize = 200;
const SUBJECT_VERSION_MAX_LEN: usize = 128;
const ROLE_MAX_LEN: usize = 128;
const ORGANIZATION_ID_MAX_LEN: usize = 128;
const USER_ID_MAX_LEN: usize = 128;
const REASON_CODE_MAX_LEN: usize = 64;
const IMPACT_SUMMARY_MAX_LEN: usize = 512;
const CLOSE_REASON_MAX_LEN: usize = 512;

/// 当前代码注册的任务类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemType {
    /// 销售单生效后的采购建单。
    ProcurementOrderCreation,
    /// 采购单财务审核。
    PurchaseOrderReview,
    /// 销售变更履约影响复核。
    SalesChangeImpactReview,
    /// 销售变更财务影响复核。
    SalesChangeFinanceReview,
    /// 卡券票款复核。
    CardFundsReview,
    /// 卡券票款差异复核。
    CardFundsDeltaReview,
    /// 归属迁移销售确认。
    OwnershipMigrationSalesConfirmation,
    /// 归属迁移财务确认。
    OwnershipMigrationFinanceConfirmation,
    /// 库存调整复核。
    InventoryAdjustmentReview,
    /// 财务纠错复核。
    FinanceCorrectionReview,
    /// 供应商结算复核。
    SupplierSettlementReview,
    /// 导入业务确认。
    ImportBusinessConfirmation,
    /// 集成结果未知。
    IntegrationResultUnknown,
    /// 业务异常。
    BusinessException,
    /// 通用单据审批任务。
    DocumentApproval,
}

/// 工作项简报关联的权威业务对象种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkItemBriefObjectKind {
    /// 销售单。
    SalesOrder,
    /// 采购确认。
    ProcurementConfirmation,
    /// 采购单。
    PurchaseOrder,
    /// 采购变更单。
    PurchaseChangeOrder,
    /// 销售变更单。
    SalesChangeOrder,
    /// 应收子账。
    ReceivableAccount,
    /// 客户回款。
    CustomerReceipt,
    /// 客户退款。
    CustomerRefund,
    /// 回款冲正。
    ReceiptReversal,
    /// 供应商付款。
    SupplierPayment,
    /// 供应商退款。
    SupplierRefund,
    /// 付款冲正。
    PaymentReversal,
    /// 库存调整。
    StockAdjustment,
    /// 供应商结算。
    SupplierSettlement,
    /// 旧数据导入批次。
    LegacyImportBatch,
    /// 集成异常任务。
    IntegrationErrorTask,
    /// 对账差异。
    ReconciliationDifference,
    /// 主数据映射任务。
    MasterMappingTask,
    /// 供应商履约订单。
    SupplierFulfillmentOrder,
    /// 供应商供给。
    SupplierOffering,
}

/// 工作项类型、业务对象类型与简报读取权限的固定关系。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkItemBriefRelation {
    /// 工作项类型。
    pub work_item_type: WorkItemType,
    /// 权威业务对象种类。
    pub object_kind: WorkItemBriefObjectKind,
    /// 持久化业务对象类型。
    pub business_object_type: &'static str,
    /// 读取该对象所需的权限。
    pub read_permission: &'static str,
}

/// 工作项责任形成时采用的岗位分离策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkItemAssignmentSeparationPolicy {
    /// 由审批运行时历史决定，通用责任入口失败关闭。
    ApprovalHistory,
    /// 排除领域提交人、经办人和既往决定人。
    DomainActors,
    /// 只要求具体角色、权限与对象参与关系。
    RoleAndParticipation,
    /// 尚无可证明策略，必须失败关闭。
    FailClosed,
}

/// 已验证可参与工作项授权计算的账号身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableWorkItemAccount {
    account_id: String,
    kind: AccountKind,
}

impl AvailableWorkItemAccount {
    /// 从统一账号主数据形成可用工作项账号。
    ///
    /// # 参数
    /// * `account` - 当前统一账号事实
    ///
    /// # 返回
    /// 账号处于可登录状态时返回稳定身份与账号类型。
    ///
    /// # 错误
    /// 账号已停用或归档时返回错误。
    pub fn from_account(account: &AccountCore) -> Result<Self> {
        if !account.can_login() {
            return Err(Error::from("工作项账号不可登录"));
        }
        Ok(Self {
            account_id: account.base.id.clone(),
            kind: account.kind,
        })
    }

    /// 从统一账号主数据形成指定类型的可用工作项账号。
    ///
    /// # 参数
    /// * `account` - 当前统一账号事实
    /// * `expected_kind` - 授权快照冻结的账号类型
    ///
    /// # 返回
    /// 账号可登录且类型未变化时返回稳定身份。
    ///
    /// # 错误
    /// 账号不可登录或类型已变化时返回错误。
    pub fn from_account_kind(account: &AccountCore, expected_kind: AccountKind) -> Result<Self> {
        let available = Self::from_account(account)?;
        if available.kind != expected_kind {
            return Err(Error::from("工作项账号类型已变化"));
        }
        Ok(available)
    }

    /// 返回稳定账号 ID。
    ///
    /// # 返回
    /// 返回统一账号主键。
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    /// 返回账号类型。
    ///
    /// # 返回
    /// 返回当前已验证账号类型。
    pub fn kind(&self) -> AccountKind {
        self.kind
    }
}

/// 业务对象允许工作项引用的权威版本集合。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkItemSubjectVersions {
    values: Vec<String>,
}

impl WorkItemSubjectVersions {
    /// 创建不限制对象版本的关系。
    ///
    /// # 返回
    /// 返回接受任意任务对象版本的空约束。
    pub fn unrestricted() -> Self {
        Self::default()
    }

    /// 创建只接受指定权威版本的关系。
    ///
    /// # 参数
    /// * `values` - 业务对象生产者允许的版本集合
    ///
    /// # 返回
    /// 返回已规范化、排序并去重的版本约束。
    ///
    /// # 错误
    /// 集合为空、任一版本为空或过长时返回错误。
    pub fn constrained(values: Vec<String>) -> Result<Self> {
        if values.is_empty() {
            return Err(Error::from("受约束的工作项对象版本不能为空"));
        }
        let mut normalized = values
            .into_iter()
            .map(|value| {
                normalize_required_text(
                    value,
                    "工作项对象版本不能为空",
                    SUBJECT_VERSION_MAX_LEN,
                    "工作项对象版本过长",
                )
            })
            .collect::<Result<Vec<_>>>()?;
        normalized.sort();
        normalized.dedup();
        Ok(Self { values: normalized })
    }

    /// 判断任务对象版本是否被当前关系接受。
    ///
    /// # 参数
    /// * `actual` - 工作项冻结的对象版本
    ///
    /// # 返回
    /// 无约束或命中权威版本集合时返回 `true`。
    pub fn accepts(&self, actual: &str) -> bool {
        self.values.is_empty() || self.values.iter().any(|expected| expected == actual)
    }

    /// 返回已规范化的权威版本集合。
    ///
    /// # 返回
    /// 无约束时返回空切片，否则返回排序去重后的版本。
    pub fn as_slice(&self) -> &[String] {
        &self.values
    }
}

const WORK_ITEM_BRIEF_RELATIONS: &[WorkItemBriefRelation] = &[
    WorkItemBriefRelation {
        work_item_type: WorkItemType::ProcurementOrderCreation,
        object_kind: WorkItemBriefObjectKind::SalesOrder,
        business_object_type: "sales_order",
        read_permission: "purchase_order:create",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::ImportBusinessConfirmation,
        object_kind: WorkItemBriefObjectKind::SalesOrder,
        business_object_type: "sales_order",
        read_permission: "sales_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::ImportBusinessConfirmation,
        object_kind: WorkItemBriefObjectKind::ProcurementConfirmation,
        business_object_type: "procurement_confirmation",
        read_permission: "procurement_confirmation:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::PurchaseOrderReview,
        object_kind: WorkItemBriefObjectKind::PurchaseOrder,
        business_object_type: "purchase_order",
        read_permission: "purchase_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::SalesChangeImpactReview,
        object_kind: WorkItemBriefObjectKind::SalesChangeOrder,
        business_object_type: "sales_change_review",
        read_permission: "sales_change_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::SalesChangeFinanceReview,
        object_kind: WorkItemBriefObjectKind::SalesChangeOrder,
        business_object_type: "sales_change_review",
        read_permission: "sales_change_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::SalesOrder,
        business_object_type: "sales_order",
        read_permission: "sales_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::CardFundsReview,
        object_kind: WorkItemBriefObjectKind::ReceivableAccount,
        business_object_type: "receivable_account",
        read_permission: "receivable_account:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::CardFundsDeltaReview,
        object_kind: WorkItemBriefObjectKind::ReceivableAccount,
        business_object_type: "receivable_account",
        read_permission: "receivable_account:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::InventoryAdjustmentReview,
        object_kind: WorkItemBriefObjectKind::StockAdjustment,
        business_object_type: "stock_adjustment",
        read_permission: "stock_adjustment:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::SupplierSettlementReview,
        object_kind: WorkItemBriefObjectKind::SupplierSettlement,
        business_object_type: "supplier_settlement_statement",
        read_permission: "supplier_settlement_statement:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::ImportBusinessConfirmation,
        object_kind: WorkItemBriefObjectKind::LegacyImportBatch,
        business_object_type: "LEGACY_IMPORT_BATCH",
        read_permission: "legacy_import_batch:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::IntegrationResultUnknown,
        object_kind: WorkItemBriefObjectKind::IntegrationErrorTask,
        business_object_type: "integration_error_task",
        read_permission: "integration_error_task:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::BusinessException,
        object_kind: WorkItemBriefObjectKind::IntegrationErrorTask,
        business_object_type: "integration_error_task",
        read_permission: "integration_error_task:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::BusinessException,
        object_kind: WorkItemBriefObjectKind::ReconciliationDifference,
        business_object_type: "reconciliation_difference",
        read_permission: "reconciliation_difference:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::IntegrationResultUnknown,
        object_kind: WorkItemBriefObjectKind::ReconciliationDifference,
        business_object_type: "reconciliation_difference",
        read_permission: "reconciliation_difference:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::BusinessException,
        object_kind: WorkItemBriefObjectKind::MasterMappingTask,
        business_object_type: "MASTER_MAPPING_TASK",
        read_permission: "master_mapping_task:list",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::IntegrationResultUnknown,
        object_kind: WorkItemBriefObjectKind::SupplierFulfillmentOrder,
        business_object_type: "SUPPLIER_FULFILLMENT_ORDER",
        read_permission: "supplier_fulfillment_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::BusinessException,
        object_kind: WorkItemBriefObjectKind::SupplierFulfillmentOrder,
        business_object_type: "SUPPLIER_FULFILLMENT_ORDER",
        read_permission: "supplier_fulfillment_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::BusinessException,
        object_kind: WorkItemBriefObjectKind::SupplierOffering,
        business_object_type: "SUPPLIER_OFFERING",
        read_permission: "supplier_offering:list",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::SalesOrder,
        business_object_type: "voucher_sales_order",
        read_permission: "sales_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::SalesChangeOrder,
        business_object_type: "sales_change_order",
        read_permission: "sales_change_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::PurchaseOrder,
        business_object_type: "purchase_order",
        read_permission: "purchase_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::PurchaseChangeOrder,
        business_object_type: "purchase_change_order",
        read_permission: "purchase_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::StockAdjustment,
        business_object_type: "stock_adjustment",
        read_permission: "stock_adjustment:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::CustomerReceipt,
        business_object_type: "customer_receipt",
        read_permission: "receivable_account:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::CustomerRefund,
        business_object_type: "customer_refund",
        read_permission: "receivable_account:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::ReceiptReversal,
        business_object_type: "receipt_reversal",
        read_permission: "receivable_account:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::SupplierPayment,
        business_object_type: "supplier_payment",
        read_permission: "purchase_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::SupplierRefund,
        business_object_type: "supplier_refund",
        read_permission: "purchase_order:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::DocumentApproval,
        object_kind: WorkItemBriefObjectKind::PaymentReversal,
        business_object_type: "payment_reversal",
        read_permission: "purchase_order:detail",
    },
];

impl WorkItemType {
    /// 返回面向用户的任务类型标签。
    ///
    /// # 返回
    /// 返回稳定中文展示名。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ProcurementOrderCreation => "采购建单",
            Self::PurchaseOrderReview => "采购单财务审核",
            Self::SalesChangeImpactReview => "销售变更履约影响复核",
            Self::SalesChangeFinanceReview => "销售变更财务影响复核",
            Self::CardFundsReview => "卡券票款复核",
            Self::CardFundsDeltaReview => "卡券票款差异复核",
            Self::OwnershipMigrationSalesConfirmation => "归属迁移销售确认",
            Self::OwnershipMigrationFinanceConfirmation => "归属迁移财务确认",
            Self::InventoryAdjustmentReview => "库存调整复核",
            Self::FinanceCorrectionReview => "财务纠错复核",
            Self::SupplierSettlementReview => "供应商结算复核",
            Self::ImportBusinessConfirmation => "导入业务确认",
            Self::IntegrationResultUnknown => "集成结果未知",
            Self::BusinessException => "业务异常",
            Self::DocumentApproval => "单据审批",
        }
    }

    /// 返回任务类型的持久化代码。
    ///
    /// # 返回
    /// 返回 `SCREAMING_SNAKE_CASE` 稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProcurementOrderCreation => "PROCUREMENT_ORDER_CREATION",
            Self::PurchaseOrderReview => "PURCHASE_ORDER_REVIEW",
            Self::SalesChangeImpactReview => "SALES_CHANGE_IMPACT_REVIEW",
            Self::SalesChangeFinanceReview => "SALES_CHANGE_FINANCE_REVIEW",
            Self::CardFundsReview => "CARD_FUNDS_REVIEW",
            Self::CardFundsDeltaReview => "CARD_FUNDS_DELTA_REVIEW",
            Self::OwnershipMigrationSalesConfirmation => "OWNERSHIP_MIGRATION_SALES_CONFIRMATION",
            Self::OwnershipMigrationFinanceConfirmation => "OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION",
            Self::InventoryAdjustmentReview => "INVENTORY_ADJUSTMENT_REVIEW",
            Self::FinanceCorrectionReview => "FINANCE_CORRECTION_REVIEW",
            Self::SupplierSettlementReview => "SUPPLIER_SETTLEMENT_REVIEW",
            Self::ImportBusinessConfirmation => "IMPORT_BUSINESS_CONFIRMATION",
            Self::IntegrationResultUnknown => "INTEGRATION_RESULT_UNKNOWN",
            Self::BusinessException => "BUSINESS_EXCEPTION",
            Self::DocumentApproval => "DOCUMENT_APPROVAL",
        }
    }

    /// 按业务对象类型解析固定简报关系。
    ///
    /// # 参数
    /// * `business_object_type` - 工作项持久化的业务对象类型
    ///
    /// # 返回
    /// 已注册组合返回对象种类与读取权限；未注册组合返回 `None`。
    pub fn brief_relation(self, business_object_type: &str) -> Option<&'static WorkItemBriefRelation> {
        WORK_ITEM_BRIEF_RELATIONS.iter().find(|relation| {
            relation.work_item_type == self && relation.business_object_type == business_object_type
        })
    }

    /// 返回全部固定工作项简报关系。
    ///
    /// # 返回
    /// 返回只读静态关系表，供授权查询形成安全对象形状。
    pub fn registered_brief_relations() -> &'static [WorkItemBriefRelation] {
        WORK_ITEM_BRIEF_RELATIONS
    }

    /// 返回当前任务类型的岗位分离策略。
    ///
    /// # 返回
    /// 返回审批历史、领域参与人、角色参与或失败关闭策略。
    pub fn assignment_separation_policy(self) -> WorkItemAssignmentSeparationPolicy {
        match self {
            Self::DocumentApproval => WorkItemAssignmentSeparationPolicy::ApprovalHistory,
            Self::ProcurementOrderCreation => WorkItemAssignmentSeparationPolicy::RoleAndParticipation,
            Self::ImportBusinessConfirmation
            | Self::PurchaseOrderReview
            | Self::SalesChangeImpactReview
            | Self::SalesChangeFinanceReview
            | Self::CardFundsReview
            | Self::CardFundsDeltaReview
            | Self::InventoryAdjustmentReview
            | Self::SupplierSettlementReview => WorkItemAssignmentSeparationPolicy::DomainActors,
            Self::IntegrationResultUnknown | Self::BusinessException => {
                WorkItemAssignmentSeparationPolicy::RoleAndParticipation
            }
            Self::OwnershipMigrationSalesConfirmation
            | Self::OwnershipMigrationFinanceConfirmation
            | Self::FinanceCorrectionReview => WorkItemAssignmentSeparationPolicy::FailClosed,
        }
    }

    /// 判断任务是否为采购建单责任。
    ///
    /// # 返回
    /// 采购建单任务返回 `true`。
    pub fn is_procurement_order_creation(self) -> bool {
        self == Self::ProcurementOrderCreation
    }

    /// 判断任务是否为通用单据审批。
    ///
    /// # 返回
    /// 单据审批任务返回 `true`。
    pub fn is_document_approval(self) -> bool {
        self == Self::DocumentApproval
    }

    /// 判断任务类型与对象类型是否属于 W29 可受控关闭关系。
    ///
    /// # 参数
    /// * `business_object_type` - 任务业务对象类型
    /// * `has_approval_step` - 是否绑定审批节点执行
    ///
    /// # 返回
    /// 仅非审批的集成异常或对账差异任务返回 `true`。
    pub fn is_w29_closable(self, business_object_type: &str, has_approval_step: bool) -> bool {
        !has_approval_step
            && matches!(
                (self, business_object_type),
                (
                    Self::IntegrationResultUnknown | Self::BusinessException,
                    "integration_error_task" | "reconciliation_difference"
                )
            )
    }
}

/// 任务生命周期状态；个人责任是否形成由 `owner_user_id` 独立表达。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemStatus {
    /// 当前仍需处理。
    #[default]
    Open,
    /// 已由强类型领域命令原子完成。
    Completed,
    /// 已按受控原因关闭。
    Closed,
}

impl WorkItemStatus {
    /// 返回面向用户的状态标签。
    ///
    /// # 返回
    /// 返回稳定中文展示名。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open => "待处理",
            Self::Completed => "已完成",
            Self::Closed => "已关闭",
        }
    }

    /// 返回状态的持久化代码。
    ///
    /// # 返回
    /// 返回 `OPEN`、`COMPLETED` 或 `CLOSED`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "OPEN",
            Self::Completed => "COMPLETED",
            Self::Closed => "CLOSED",
        }
    }
}

impl DocumentState for WorkItemStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Open => &[Self::Completed, Self::Closed],
            Self::Completed | Self::Closed => &[],
        }
    }
}

/// 单据审批任务进入决定编排前的纯领域校验失败原因。
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecisionTaskError {
    /// 当前任务不是单据审批任务。
    #[error("不是单据审批任务")]
    NotDocumentApproval,
    /// 操作人不是当前个人责任人。
    #[error("无权执行该审批动作")]
    NotCurrentOwner,
    /// 任务已经完成或关闭。
    #[error("APPROVAL_TASK_NOT_OPEN")]
    NotOpen,
    /// 调用方持有的任务版本已经过期。
    #[error("任务版本已变化，请刷新后重试")]
    VersionConflict,
    /// 审批任务缺少节点执行引用。
    #[error("APPROVAL_TASK_NOT_OPEN")]
    MissingExecution,
}

/// 当前个人责任的已注册形成来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssignmentSource {
    /// 独立任务的固定系统规则。
    SystemRule,
    /// 管理员受控转交。
    AdminReassign,
    /// 审批运行时指定到人。
    ApprovalRuntime,
}

impl AssignmentSource {
    /// 返回责任来源的持久化代码。
    ///
    /// # 返回
    /// 返回已注册的稳定来源代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SystemRule => "SYSTEM_RULE",
            Self::AdminReassign => "ADMIN_REASSIGN",
            Self::ApprovalRuntime => "APPROVAL_RUNTIME",
        }
    }
}

/// 待办优先级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemPriority {
    /// 紧急。
    Urgent,
    /// 高。
    High,
    /// 普通。
    Normal,
    /// 低。
    Low,
}

impl WorkItemPriority {
    /// 返回面向用户的优先级标签。
    ///
    /// # 返回
    /// 返回稳定中文展示名。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Urgent => "紧急",
            Self::High => "高",
            Self::Normal => "普通",
            Self::Low => "低",
        }
    }

    /// 返回优先级的持久化代码。
    ///
    /// # 返回
    /// 返回小写稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Urgent => "urgent",
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }
}

/// 通用单据审批任务的创建数据。责任人、角色、组织和执行 ID 均必填。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocumentApprovalWorkItemData {
    /// 当前节点执行。
    pub approval_node_execution_id: ApprovalNodeExecutionId,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 被审批的冻结提交版本。
    pub subject_version: String,
    /// 合同签署的责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前实例审批人。
    pub owner_user_id: String,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
}

/// 创建任务所需的责任与业务对象快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemData {
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 被处理的不可变提交或业务版本。
    pub subject_version: String,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人；创建开放任务时必填。
    pub owner_user_id: String,
    /// 初始责任来源。
    pub assignment_source: AssignmentSource,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 业务影响摘要。
    pub impact_summary: Option<String>,
}

/// 受控关闭任务的数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkItemCloseData {
    /// 不可为空的关闭原因。
    pub close_reason: String,
}

/// 当前人工责任事实。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct WorkItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 类型化审批节点执行；审批任务必填。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_node_execution_id: Option<ApprovalNodeExecutionId>,
    /// 业务对象类型。
    pub business_object_type: String,
    /// 业务对象 ID。
    pub business_object_id: String,
    /// 服务端冻结的可选责任维度；普通任务为空，存在时参与开放任务唯一性。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    responsibility_key: Option<String>,
    /// 服务端冻结的稳定业务行范围；普通任务为空。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    responsibility_scope_ids: Vec<String>,
    /// 被处理的不可变提交或业务版本。
    pub subject_version: String,
    /// 生命周期状态。
    pub status: WorkItemStatus,
    /// 责任角色。
    pub owner_role: String,
    /// 责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人；开放任务必填。
    pub owner_user_id: Option<String>,
    /// 曾形成个人责任的用户 ID；只追加、去重，退回与终态动作均保留。
    pub responsibility_actor_ids: Vec<String>,
    /// 当前或最近一次责任来源。
    pub assignment_source: AssignmentSource,
    /// 首次形成个人责任的时间。
    pub assigned_at: Option<Instant>,
    /// 首次正式处理时间。
    pub started_at: Option<Instant>,
    /// 当前个人责任生效时间。
    pub current_assignment_at: Option<Instant>,
    /// 最近一次非只读活动时间。
    pub last_activity_at: Option<Instant>,
    /// 优先级。
    pub priority: WorkItemPriority,
    /// 时限。
    pub due_at: Option<Instant>,
    /// 产生原因代码。
    pub reason_code: Option<String>,
    /// 业务影响摘要。
    pub impact_summary: Option<String>,
    /// 正式完成时间。
    pub completed_at: Option<Instant>,
    /// 正式完成人。
    pub completed_by: Option<String>,
    /// 受控关闭时间。
    pub closed_at: Option<Instant>,
    /// 受控关闭操作人。
    pub closed_by: Option<String>,
    /// 受控关闭原因。
    pub close_reason: Option<String>,
}

impl WorkItem {
    /// 创建任务并建立初始责任事实。
    ///
    /// 开放任务必须给出唯一责任人，并立即形成 `assigned_at` 与
    /// `current_assignment_at`。
    ///
    /// # 错误
    /// 必填字段为空、字段超长或缺少个人责任人时返回错误。
    pub fn new(id: WorkItemId, data: WorkItemData) -> Result<Self> {
        Self::new_at_with_optional_responsibility(id, data, None, Vec::new(), Instant::now())
    }

    /// 创建带服务端责任维度的任务。
    ///
    /// 责任维度在创建时规范化并冻结；后续转交不得修改。
    /// 客户端输入不得直接调用本入口，应用服务只能传入已注册的固定维度。
    ///
    /// # 错误
    /// 责任维度为空、字段超长，或任务基础数据无效时返回错误。
    pub fn new_with_responsibility_key(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: impl Into<String>,
    ) -> Result<Self> {
        let responsibility_key = normalize_responsibility_key(responsibility_key.into())?;
        Self::new_at_with_optional_responsibility(
            id,
            data,
            Some(responsibility_key),
            Vec::new(),
            Instant::now(),
        )
    }

    /// 创建带服务端责任维度与稳定业务行范围的任务。
    ///
    /// # 参数
    /// * `id` - 任务主键
    /// * `data` - 任务责任与业务对象快照
    /// * `responsibility_key` - 参与开放唯一性的服务端稳定键
    /// * `scope_ids` - 已由服务端解析、跨版本稳定的业务行 ID
    ///
    /// # 返回
    /// 返回责任键和责任行集合均已规范化、排序并冻结的开放任务。
    ///
    /// # 错误
    /// 责任键或行 ID 为空、字段过长、行数超过上限，或任务基础数据无效时返回错误。
    pub fn new_with_responsibility_scope(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: impl Into<String>,
        scope_ids: Vec<String>,
    ) -> Result<Self> {
        let responsibility_key = normalize_responsibility_key(responsibility_key.into())?;
        let scope_ids = normalize_responsibility_scope(scope_ids)?;
        Self::new_at_with_optional_responsibility(
            id,
            data,
            Some(responsibility_key),
            scope_ids,
            Instant::now(),
        )
    }

    /// 使用确定时间创建任务，供事务编排和确定性测试使用。
    ///
    /// # 错误
    /// 必填字段为空、字段超长或分派模式与个人责任不匹配时返回错误。
    pub fn new_at(id: WorkItemId, data: WorkItemData, at: Instant) -> Result<Self> {
        Self::new_at_with_optional_responsibility(id, data, None, Vec::new(), at)
    }

    /// 创建指定到人的单据审批任务。
    ///
    /// 必须同时提供非空责任人、角色、组织、审批运行时来源和节点执行 ID。
    ///
    /// # 参数
    /// * `id` - 任务主键
    /// * `data` - 审批任务数据
    /// * `at` - 创建时间
    ///
    /// # 错误
    /// 任一必填责任字段为空或超长时返回错误。
    pub fn new_document_approval(
        id: WorkItemId,
        data: DocumentApprovalWorkItemData,
        at: Instant,
    ) -> Result<Self> {
        let owner_user_id = normalize_required_text(
            data.owner_user_id,
            "审批任务责任人不能为空",
            USER_ID_MAX_LEN,
            "审批任务责任人过长",
        )?;
        let generic = WorkItemData {
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            business_object_type: data.business_object_type,
            business_object_id: data.business_object_id,
            subject_version: data.subject_version,
            owner_role: data.owner_role,
            owner_organization_id: data.owner_organization_id,
            owner_user_id: owner_user_id.clone(),
            assignment_source: AssignmentSource::ApprovalRuntime,
            priority: data.priority,
            due_at: data.due_at,
            reason_code: None,
            impact_summary: None,
        };
        let mut item = Self::new_at_with_optional_responsibility(id, generic, None, Vec::new(), at)?;
        item.work_item_type = WorkItemType::DocumentApproval;
        item.approval_node_execution_id = Some(data.approval_node_execution_id);
        item.assignment_source = AssignmentSource::ApprovalRuntime;
        Ok(item)
    }

    /// 使用已规范化的可选责任键和责任范围创建任务。
    ///
    /// # 参数
    /// * `id` - 任务主键
    /// * `data` - 任务基础数据
    /// * `responsibility_key` - 可选服务端责任键
    /// * `responsibility_scope_ids` - 已规范化的稳定业务行集合
    /// * `at` - 责任形成时间
    ///
    /// # 返回
    /// 返回初始状态为开放且已指定到人的任务。
    ///
    /// # 错误
    /// 任务基础数据不合法或误用通用路径创建审批任务时返回错误。
    fn new_at_with_optional_responsibility(
        id: WorkItemId,
        data: WorkItemData,
        responsibility_key: Option<String>,
        responsibility_scope_ids: Vec<String>,
        at: Instant,
    ) -> Result<Self> {
        let normalized = NormalizedWorkItemData::try_from(data)?;
        if normalized.work_item_type == WorkItemType::DocumentApproval {
            return Err(Error::from("单据审批任务必须使用专用构造路径"));
        }
        if normalized.work_item_type == WorkItemType::ProcurementOrderCreation
            && (responsibility_key.is_none() || responsibility_scope_ids.is_empty())
        {
            return Err(Error::from("采购建单任务必须冻结责任键和责任行范围"));
        }
        if normalized.work_item_type != WorkItemType::ProcurementOrderCreation
            && !responsibility_scope_ids.is_empty()
        {
            return Err(Error::from("只有采购建单任务可以冻结责任行范围"));
        }
        let responsibility_actor_ids = vec![normalized.owner_user_id.clone()];
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            work_item_type: normalized.work_item_type,
            approval_node_execution_id: None,
            business_object_type: normalized.business_object_type,
            business_object_id: normalized.business_object_id,
            responsibility_key,
            responsibility_scope_ids,
            subject_version: normalized.subject_version,
            status: WorkItemStatus::Open,
            owner_role: normalized.owner_role,
            owner_organization_id: normalized.owner_organization_id,
            owner_user_id: Some(normalized.owner_user_id),
            responsibility_actor_ids,
            assignment_source: normalized.assignment_source,
            assigned_at: Some(at),
            started_at: None,
            current_assignment_at: Some(at),
            last_activity_at: None,
            priority: normalized.priority,
            due_at: normalized.due_at,
            reason_code: normalized.reason_code,
            impact_summary: normalized.impact_summary,
            completed_at: None,
            completed_by: None,
            closed_at: None,
            closed_by: None,
            close_reason: None,
        })
    }

    /// 返回创建时冻结的责任维度。
    ///
    /// # 返回
    /// 普通任务返回 `None`；采用多责任维度开放唯一性的任务返回固定键。
    pub fn responsibility_key(&self) -> Option<&str> {
        self.responsibility_key.as_deref()
    }

    /// 返回创建时冻结的稳定业务行范围。
    ///
    /// # 返回
    /// 返回按稳定 ID 排序并去重的只读切片；普通任务返回空切片。
    pub fn responsibility_scope_ids(&self) -> &[String] {
        &self.responsibility_scope_ids
    }

    /// 更新开放任务的业务影响摘要。
    ///
    /// # 参数
    /// * `impact_summary` - 面向用户的最新业务影响；空白值会规范化为空
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 任务非开放或摘要超过长度上限时返回错误。
    pub fn update_impact_summary(&mut self, impact_summary: Option<String>) -> Result<()> {
        self.ensure_open()?;
        self.impact_summary = normalize_optional_text(impact_summary, "影响摘要", IMPACT_SUMMARY_MAX_LEN)?;
        Ok(())
    }

    /// 在业务需求已由系统事实完全满足时自动完成开放任务。
    ///
    /// # 参数
    /// * `at` - 系统确认需求归零的时间
    ///
    /// # 返回
    /// 自动完成成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 单据审批任务或非开放任务不能使用本入口。
    pub fn complete_when_requirement_satisfied(&mut self, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        if self.work_item_type != WorkItemType::ProcurementOrderCreation {
            return Err(Error::from("只有采购建单任务可以按需求归零自动完成"));
        }
        self.ensure_open()?;
        self.status = WorkItemStatus::Completed;
        self.completed_at = Some(at);
        self.completed_by = Some("__system__".to_string());
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 为重新释放的采购需求创建一条新的开放任务。
    ///
    /// # 参数
    /// * `id` - 新任务主键
    /// * `subject_version` - 重新释放时的销售当前版本
    /// * `impact_summary` - 重新释放后的剩余数量摘要
    ///
    /// # 返回
    /// 返回复制当前责任人、责任来源和冻结行范围的新开放任务。
    ///
    /// # 错误
    /// 当前任务不是采购建单终态、缺少责任人或冻结责任事实时返回错误。
    ///
    /// # 关键业务约束
    /// 历史终态任务保持不变；重新释放必须形成新任务身份。
    pub fn successor_for_released_requirement(
        &self,
        id: WorkItemId,
        subject_version: String,
        impact_summary: Option<String>,
    ) -> Result<Self> {
        if self.work_item_type != WorkItemType::ProcurementOrderCreation {
            return Err(Error::from("只有采购建单任务可以创建释放后继任务"));
        }
        if self.status == WorkItemStatus::Open {
            return Err(Error::from("开放采购建单任务不能创建释放后继任务"));
        }
        let responsibility_key = self
            .responsibility_key()
            .ok_or_else(|| Error::from("历史采购建单任务缺少责任键"))?
            .to_string();
        let owner_user_id = self
            .owner_user_id
            .clone()
            .ok_or_else(|| Error::from("历史采购建单任务缺少具体责任人"))?;
        Self::new_with_responsibility_scope(
            id,
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: self.business_object_type.clone(),
                business_object_id: self.business_object_id.clone(),
                subject_version,
                owner_role: self.owner_role.clone(),
                owner_organization_id: self.owner_organization_id.clone(),
                owner_user_id,
                assignment_source: self.assignment_source,
                priority: self.priority,
                due_at: self.due_at,
                reason_code: Some("PROCUREMENT_QUANTITY_RELEASED".to_string()),
                impact_summary,
            },
            responsibility_key,
            self.responsibility_scope_ids.clone(),
        )
    }

    /// 记录当前责任人的首次处理或后续非终结活动。
    ///
    /// `started_at` 只在第一次调用时写入；后续调用仅推进 `last_activity_at`。
    ///
    /// # 错误
    /// 任务非开放、没有个人责任或操作人不是当前责任人时返回错误。
    pub fn record_activity(&mut self, actor_id: &str, at: Instant) -> Result<()> {
        self.ensure_current_owner(actor_id)?;
        self.started_at.get_or_insert(at);
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 转交开放任务的当前个人责任。
    ///
    /// 首次分派时间只在此前从未形成个人责任时写入；首次处理时间保持不变。
    /// 调用方必须在应用层重新校验目标任职、角色、数据范围与岗位分离。
    ///
    /// # 错误
    /// 任务非开放或目标用户为空、超长时返回错误。
    pub fn reassign(&mut self, target_user_id: impl Into<String>, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        self.assign_to(target_user_id, AssignmentSource::AdminReassign, at)
    }

    /// 由强类型领域命令完成当前开放任务。
    ///
    /// 本方法只形成任务事实；调用方必须把正式领域事实、审批推进和本实体写入
    /// 放在同一事务。完成动作同时按 `if_null` 语义形成首次处理时间。
    ///
    /// # 错误
    /// 任务非开放、没有个人责任或执行人不是当前责任人时返回错误。
    pub fn complete_by_domain_command(&mut self, completed_by: impl Into<String>, at: Instant) -> Result<()> {
        self.ensure_generic_mutation()?;
        self.complete_open(completed_by, at)
    }

    /// 校验单据审批任务可由当前操作人按给定版本进入决定编排。
    ///
    /// # 参数
    /// * `actor_id` - 当前已认证操作人 ID
    /// * `expected_version` - 调用方期望的任务乐观锁版本
    ///
    /// # 返回
    /// 返回任务绑定的审批节点执行 ID。
    ///
    /// # 错误
    /// 任务类型、责任人、开放状态、版本或执行引用不满足时返回对应错误。
    ///
    /// # 关键业务约束
    /// 校验顺序固定为任务类型、当前责任人、开放状态、版本和执行引用，避免 Service 重复状态判断。
    pub fn approval_execution_for_decision(
        &self,
        actor_id: &str,
        expected_version: u64,
    ) -> std::result::Result<ApprovalNodeExecutionId, ApprovalDecisionTaskError> {
        if self.work_item_type != WorkItemType::DocumentApproval {
            return Err(ApprovalDecisionTaskError::NotDocumentApproval);
        }
        if self.owner_user_id.as_deref() != Some(actor_id) {
            return Err(ApprovalDecisionTaskError::NotCurrentOwner);
        }
        if self.status != WorkItemStatus::Open {
            return Err(ApprovalDecisionTaskError::NotOpen);
        }
        if self.base.version != expected_version {
            return Err(ApprovalDecisionTaskError::VersionConflict);
        }
        self.approval_node_execution_id
            .clone()
            .ok_or(ApprovalDecisionTaskError::MissingExecution)
    }

    /// 由审批运行时完成当前开放的单据审批任务。
    ///
    /// # 错误
    /// 不是单据审批任务、任务非开放或执行人不是当前责任人时返回错误。
    pub fn complete_by_approval_runtime(
        &mut self,
        completed_by: impl Into<String>,
        at: Instant,
    ) -> Result<()> {
        self.ensure_document_approval()?;
        self.complete_open(completed_by, at)
    }

    /// 以受控原因关闭开放任务。
    ///
    /// 调用方必须先执行任务类型关闭策略与专门权限校验；关闭不会完成业务动作。
    ///
    /// # 错误
    /// 任务非开放、操作人或关闭原因为空、超长时返回错误。
    pub fn close(
        &mut self,
        closed_by: impl Into<String>,
        data: WorkItemCloseData,
        at: Instant,
    ) -> Result<()> {
        self.ensure_generic_mutation()?;
        self.close_open(closed_by, data, at)
    }

    /// 由审批运行时关闭当前开放的单据审批任务。
    ///
    /// # 错误
    /// 不是单据审批任务、任务非开放或关闭数据非法时返回错误。
    pub fn close_by_approval_runtime(
        &mut self,
        closed_by: impl Into<String>,
        data: WorkItemCloseData,
        at: Instant,
    ) -> Result<()> {
        self.ensure_document_approval()?;
        self.close_open(closed_by, data, at)
    }

    /// 随审批实例取消关闭当前开放任务。
    ///
    /// 只有带节点执行引用的单据审批任务可以使用本入口；返回实体保持原持久化
    /// 版本，供仓储以 `OPEN + version + execution_id` 执行 CAS。
    ///
    /// # 参数
    /// * `closed_by` - 撤回审批的操作人
    /// * `reason` - 不可为空的撤回原因
    /// * `at` - 取消发生时间
    ///
    /// # 返回
    /// 返回已进入 `CLOSED` 的任务快照。
    ///
    /// # 错误
    /// 任务类型错误、缺少节点执行、任务非开放或关闭字段非法时返回错误。
    pub fn close_for_approval_cancellation(
        mut self,
        closed_by: impl Into<String>,
        reason: impl Into<String>,
        at: Instant,
    ) -> Result<Self> {
        self.ensure_document_approval()?;
        if self.approval_node_execution_id.is_none() {
            return Err(Error::from("单据审批任务缺少节点执行引用"));
        }
        self.close_open(
            closed_by,
            WorkItemCloseData {
                close_reason: reason.into(),
            },
            at,
        )?;
        Ok(self)
    }

    /// 批量形成审批取消所需的已关闭任务快照。
    ///
    /// 任一任务不满足关闭规则时整体返回错误，不产生部分持久化结果；空集合用于
    /// 已受阻且没有开放任务的合法取消路径。
    ///
    /// # 参数
    /// * `items` - 当前节点执行关联的开放审批任务
    /// * `closed_by` - 撤回审批的操作人
    /// * `reason` - 不可为空的撤回原因
    /// * `at` - 取消发生时间
    ///
    /// # 返回
    /// 返回与输入顺序一致的已关闭任务快照。
    ///
    /// # 错误
    /// 任一任务不满足 [`Self::close_for_approval_cancellation`] 规则时返回错误。
    pub fn close_all_for_approval_cancellation(
        items: Vec<Self>,
        closed_by: &str,
        reason: &str,
        at: Instant,
    ) -> Result<Vec<Self>> {
        items
            .into_iter()
            .map(|item| item.close_for_approval_cancellation(closed_by, reason, at))
            .collect()
    }

    /// 应用开放任务的受控关闭字段。
    ///
    /// # 参数
    /// * `closed_by` - 关闭操作人
    /// * `data` - 已选择的关闭原因
    /// * `at` - 关闭时间
    ///
    /// # 返回
    /// 成功时无返回值。
    ///
    /// # 错误
    /// 任务非开放、操作人或关闭原因非法时返回错误。
    fn close_open(
        &mut self,
        closed_by: impl Into<String>,
        data: WorkItemCloseData,
        at: Instant,
    ) -> Result<()> {
        self.ensure_open()?;
        let closed_by = normalize_required_text(
            closed_by.into(),
            "关闭操作人不能为空",
            USER_ID_MAX_LEN,
            "关闭操作人过长",
        )?;
        let close_reason = normalize_required_text(
            data.close_reason,
            "关闭原因不能为空",
            CLOSE_REASON_MAX_LEN,
            "关闭原因过长",
        )?;
        self.status = WorkItemStatus::Closed;
        self.closed_at = Some(at);
        self.closed_by = Some(closed_by);
        self.close_reason = Some(close_reason);
        Ok(())
    }

    /// 返回任务是否已进入不可逆终态。
    ///
    /// # 返回
    /// `COMPLETED` 或 `CLOSED` 时返回 `true`。
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, WorkItemStatus::Completed | WorkItemStatus::Closed)
    }

    /// 判断给定用户是否是开放任务的当前个人责任人。
    ///
    /// # 返回
    /// 任务开放且责任人与给定用户相同时返回 `true`。
    pub fn is_owned_by(&self, user_id: &str) -> bool {
        self.status == WorkItemStatus::Open && self.owner_user_id.as_deref() == Some(user_id)
    }

    /// 校验任务可以进入通用责任变更或关闭入口。
    ///
    /// # 返回
    /// 非单据审批任务返回 `Ok(())`。
    ///
    /// # 错误
    /// 单据审批任务或带审批节点执行引用时返回固定禁止错误。
    pub fn ensure_generic_responsibility_mutation(&self) -> Result<()> {
        if self.approval_node_execution_id.is_some() || self.work_item_type.is_document_approval() {
            return Err(Error::from("APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN"));
        }
        Ok(())
    }

    /// 判断任务是否绑定指定业务对象身份。
    ///
    /// # 参数
    /// * `business_object_type` - 期望业务对象类型
    /// * `business_object_id` - 期望业务对象 ID
    ///
    /// # 返回
    /// 类型与稳定 ID 均匹配时返回 `true`。
    pub fn matches_business_object(&self, business_object_type: &str, business_object_id: &str) -> bool {
        self.business_object_type == business_object_type && self.business_object_id == business_object_id
    }

    /// 判断任务是否冻结指定业务对象版本。
    ///
    /// # 参数
    /// * `subject_version` - 权威对象版本
    ///
    /// # 返回
    /// 与任务冻结版本一致时返回 `true`。
    pub fn matches_subject_version(&self, subject_version: &str) -> bool {
        self.subject_version == subject_version
    }

    /// 判断任务是否属于 W29 可受控关闭关系。
    ///
    /// # 返回
    /// 非审批的集成异常或对账差异任务返回 `true`。
    pub fn is_w29_closable(&self) -> bool {
        self.work_item_type.is_w29_closable(
            &self.business_object_type,
            self.approval_node_execution_id.is_some(),
        )
    }

    /// 判断本任务可否作为另一 W29 任务的正式替代任务。
    ///
    /// # 参数
    /// * `current` - 待关闭的当前任务
    ///
    /// # 返回
    /// 本任务不同于当前任务、仍开放、同任务类型且同对象类别时返回 `true`。
    pub fn is_w29_replacement_for(&self, current: &Self) -> bool {
        self.base.id != current.base.id
            && self.status == WorkItemStatus::Open
            && self.is_w29_closable()
            && self.work_item_type == current.work_item_type
            && self.business_object_type == current.business_object_type
    }

    fn complete_open(&mut self, completed_by: impl Into<String>, at: Instant) -> Result<()> {
        let completed_by = normalize_required_text(
            completed_by.into(),
            "完成执行人不能为空",
            USER_ID_MAX_LEN,
            "完成执行人过长",
        )?;
        self.ensure_current_owner(&completed_by)?;
        self.started_at.get_or_insert(at);
        self.status = WorkItemStatus::Completed;
        self.completed_at = Some(at);
        self.completed_by = Some(completed_by);
        Ok(())
    }

    fn ensure_generic_mutation(&self) -> Result<()> {
        self.ensure_generic_responsibility_mutation()
    }

    fn ensure_document_approval(&self) -> Result<()> {
        if self.work_item_type != WorkItemType::DocumentApproval {
            return Err(Error::from("只有单据审批任务可以由审批运行时完成或关闭"));
        }
        Ok(())
    }

    fn ensure_open(&self) -> Result<()> {
        if self.status == WorkItemStatus::Open {
            return Ok(());
        }
        Err(Error::from("只有开放任务可以执行责任动作"))
    }

    fn ensure_current_owner(&self, actor_id: &str) -> Result<()> {
        self.ensure_open()?;
        if self.owner_user_id.as_deref() == Some(actor_id) {
            return Ok(());
        }
        Err(Error::from("只有当前责任人可以处理任务"))
    }

    fn assign_to(
        &mut self,
        target_user_id: impl Into<String>,
        source: AssignmentSource,
        at: Instant,
    ) -> Result<()> {
        self.ensure_open()?;
        let target_user_id = normalize_required_text(
            target_user_id.into(),
            "目标责任人不能为空",
            USER_ID_MAX_LEN,
            "目标责任人过长",
        )?;
        self.assigned_at.get_or_insert(at);
        self.record_responsibility_actor(&target_user_id);
        self.owner_user_id = Some(target_user_id);
        self.current_assignment_at = Some(at);
        self.assignment_source = source;
        self.last_activity_at = Some(at);
        Ok(())
    }

    /// 追加首次出现的个人责任人，保留稳定的责任形成顺序。
    fn record_responsibility_actor(&mut self, actor_id: &str) {
        if !self.responsibility_actor_ids.iter().any(|id| id == actor_id) {
            self.responsibility_actor_ids.push(actor_id.to_string());
        }
    }
}

/// 规范化参与开放任务唯一性的服务端责任键。
///
/// # 参数
/// * `responsibility_key` - 原始责任键
///
/// # 返回
/// 返回去除首尾空白后的非空责任键。
///
/// # 错误
/// 责任键为空或超过长度上限时返回错误。
fn normalize_responsibility_key(responsibility_key: String) -> Result<String> {
    normalize_required_text(
        responsibility_key,
        "责任维度不能为空",
        RESPONSIBILITY_KEY_MAX_LEN,
        "责任维度过长",
    )
}

/// 规范化服务端冻结的稳定业务行范围。
///
/// # 参数
/// * `scope_ids` - 原始稳定业务行 ID 集合
///
/// # 返回
/// 返回逐项去空白、排序并去重后的非空集合。
///
/// # 错误
/// 集合为空、超过最大行数，或任一 ID 为空或过长时返回错误。
fn normalize_responsibility_scope(scope_ids: Vec<String>) -> Result<Vec<String>> {
    if scope_ids.is_empty() || scope_ids.len() > RESPONSIBILITY_SCOPE_MAX_ITEMS {
        return Err(Error::from("责任范围行数必须在1-200之间"));
    }
    let mut normalized = scope_ids
        .into_iter()
        .map(|scope_id| {
            normalize_required_text(
                scope_id,
                "责任范围行不能为空",
                OBJECT_ID_MAX_LEN,
                "责任范围行过长",
            )
        })
        .collect::<Result<Vec<_>>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

struct NormalizedWorkItemData {
    work_item_type: WorkItemType,
    business_object_type: String,
    business_object_id: String,
    subject_version: String,
    owner_role: String,
    owner_organization_id: String,
    owner_user_id: String,
    assignment_source: AssignmentSource,
    priority: WorkItemPriority,
    due_at: Option<Instant>,
    reason_code: Option<String>,
    impact_summary: Option<String>,
}

impl TryFrom<WorkItemData> for NormalizedWorkItemData {
    type Error = Error;

    fn try_from(data: WorkItemData) -> Result<Self> {
        Ok(Self {
            work_item_type: data.work_item_type,
            business_object_type: normalize_required_text(
                data.business_object_type,
                "业务对象类型不能为空",
                OBJECT_TYPE_MAX_LEN,
                "业务对象类型过长",
            )?,
            business_object_id: normalize_required_text(
                data.business_object_id,
                "业务对象ID不能为空",
                OBJECT_ID_MAX_LEN,
                "业务对象ID过长",
            )?,
            subject_version: normalize_required_text(
                data.subject_version,
                "对象版本不能为空",
                SUBJECT_VERSION_MAX_LEN,
                "对象版本过长",
            )?,
            owner_role: normalize_required_text(
                data.owner_role,
                "责任角色不能为空",
                ROLE_MAX_LEN,
                "责任角色过长",
            )?,
            owner_organization_id: normalize_required_text(
                data.owner_organization_id,
                "责任组织不能为空",
                ORGANIZATION_ID_MAX_LEN,
                "责任组织过长",
            )?,
            owner_user_id: normalize_required_text(
                data.owner_user_id,
                "责任人不能为空",
                USER_ID_MAX_LEN,
                "责任人过长",
            )?,
            assignment_source: data.assignment_source,
            priority: data.priority,
            due_at: data.due_at,
            reason_code: normalize_optional_text(data.reason_code, "原因代码", REASON_CODE_MAX_LEN)?,
            impact_summary: normalize_optional_text(data.impact_summary, "影响摘要", IMPACT_SUMMARY_MAX_LEN)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApprovalDecisionTaskError, AssignmentSource, AvailableWorkItemAccount, WorkItem,
        WorkItemAssignmentSeparationPolicy, WorkItemBriefObjectKind, WorkItemData, WorkItemPriority,
        WorkItemStatus, WorkItemSubjectVersions, WorkItemType,
    };
    use crate::common::state::ensure_transition;
    use crate::common::time::Instant;
    use crate::ids::WorkItemId;
    use crate::{AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Secret};

    /// 构造独立任务的最小测试数据。
    ///
    /// # 返回
    /// 返回带可规范化空白和固定责任人的输入。
    fn direct_data() -> WorkItemData {
        WorkItemData {
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            business_object_type: " LEGACY_IMPORT_BATCH ".to_string(),
            business_object_id: " batch-1 ".to_string(),
            subject_version: " v3 ".to_string(),
            owner_role: " sales ".to_string(),
            owner_organization_id: " org-1 ".to_string(),
            owner_user_id: " alice ".to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: Some(Instant::from_unix_secs(1_700_086_400)),
            reason_code: Some("IMPORT_READY".to_string()),
            impact_summary: Some(" 待确认导入范围 ".to_string()),
        }
    }

    /// 构造绑定固定节点执行的开放审批任务。
    ///
    /// # 返回
    /// 返回责任人为 `alice`、执行为 `exec-1` 的任务。
    fn approval_item(id: &str) -> WorkItem {
        WorkItem::new_document_approval(
            WorkItemId::new(id),
            super::DocumentApprovalWorkItemData {
                approval_node_execution_id: bpm::ApprovalNodeExecutionId::new("exec-1"),
                business_object_type: "stock_adjustment".into(),
                business_object_id: "adj-1".into(),
                subject_version: "1".into(),
                owner_role: "stock_adjustment_approver".into(),
                owner_organization_id: "org-1".into(),
                owner_user_id: " alice ".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap()
    }

    fn account(status: AccountStatus) -> AccountCore {
        AccountCore::new(
            "account-1".to_string(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new("worker").unwrap(), "password123").unwrap(),
                name: "处理人".to_string(),
                kind: AccountKind::Admin,
                status,
                email: None,
                phone: None,
                avatar: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn account_and_subject_version_value_objects_fail_closed() {
        let active = account(AccountStatus::Active);
        let available = AvailableWorkItemAccount::from_account_kind(&active, AccountKind::Admin).unwrap();
        assert_eq!(available.account_id(), "account-1");
        assert_eq!(available.kind(), AccountKind::Admin);
        assert!(AvailableWorkItemAccount::from_account(&account(AccountStatus::Suspended)).is_err());

        let versions = WorkItemSubjectVersions::constrained(vec![
            "v2".to_string(),
            " v1 ".to_string(),
            "v2".to_string(),
        ])
        .unwrap();
        assert_eq!(versions.as_slice(), &["v1".to_string(), "v2".to_string()]);
        assert!(versions.accepts("v1"));
        assert!(!versions.accepts("v3"));
        assert!(WorkItemSubjectVersions::constrained(vec![" ".to_string()]).is_err());
        assert!(WorkItemSubjectVersions::unrestricted().accepts("any"));
    }

    #[test]
    fn brief_relations_and_assignment_policies_are_entity_owned() {
        let relation = WorkItemType::DocumentApproval
            .brief_relation("stock_adjustment")
            .unwrap();
        assert_eq!(relation.object_kind, WorkItemBriefObjectKind::StockAdjustment);
        assert_eq!(relation.read_permission, "stock_adjustment:detail");
        assert!(WorkItemType::DocumentApproval.brief_relation("unknown").is_none());
        assert_eq!(
            WorkItemType::PurchaseOrderReview.assignment_separation_policy(),
            WorkItemAssignmentSeparationPolicy::DomainActors
        );
        assert_eq!(
            WorkItemType::DocumentApproval.assignment_separation_policy(),
            WorkItemAssignmentSeparationPolicy::ApprovalHistory
        );
    }

    #[test]
    fn open_task_requires_personal_owner() {
        let item = WorkItem::new_at(
            WorkItemId::new("wi-1"),
            direct_data(),
            Instant::from_unix_secs(100),
        )
        .unwrap();
        assert_eq!(item.status, WorkItemStatus::Open);
        assert_eq!(item.owner_user_id.as_deref(), Some("alice"));
        assert_eq!(item.responsibility_actor_ids, vec!["alice".to_string()]);
        let missing = WorkItemData {
            owner_user_id: "   ".to_string(),
            ..direct_data()
        };
        assert!(WorkItem::new_at(WorkItemId::new("wi-2"), missing, Instant::from_unix_secs(100)).is_err());
    }

    #[test]
    fn reassign_and_complete_preserve_first_times() {
        let first = Instant::from_unix_secs(100);
        let mut item = WorkItem::new_at(WorkItemId::new("wi-1"), direct_data(), first).unwrap();
        item.record_activity("alice", Instant::from_unix_secs(110))
            .unwrap();
        item.reassign("bob", Instant::from_unix_secs(130)).unwrap();
        item.complete_by_domain_command("bob", Instant::from_unix_secs(150))
            .unwrap();
        assert_eq!(item.assigned_at, Some(first));
        assert_eq!(item.started_at, Some(Instant::from_unix_secs(110)));
        assert_eq!(item.owner_user_id.as_deref(), Some("bob"));
        assert_eq!(item.assignment_source, AssignmentSource::AdminReassign);
        assert!(item.is_terminal());
    }

    #[test]
    fn procurement_task_cannot_bypass_frozen_scope_constructor() {
        let data = WorkItemData {
            work_item_type: WorkItemType::ProcurementOrderCreation,
            ..direct_data()
        };
        assert!(WorkItem::new_at(
            WorkItemId::new("wi-procurement"),
            data.clone(),
            Instant::from_unix_secs(100),
        )
        .is_err());
        assert!(WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-procurement-key"),
            data,
            "sales-lines:key",
        )
        .is_err());
    }

    #[test]
    fn responsibility_scope_is_normalized_and_system_completion_preserves_history() {
        let mut item = WorkItem::new_with_responsibility_scope(
            WorkItemId::new("wi-procurement"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                ..direct_data()
            },
            " sales-lines:key ",
            vec![" line-b ".to_string(), "line-a".to_string(), "line-a".to_string()],
        )
        .unwrap();
        assert_eq!(item.responsibility_key(), Some("sales-lines:key"));
        assert_eq!(
            item.responsibility_scope_ids(),
            &["line-a".to_string(), "line-b".to_string()]
        );
        item.update_impact_summary(Some(" 剩余 6 件待采购 ".to_string()))
            .unwrap();
        assert_eq!(item.impact_summary.as_deref(), Some("剩余 6 件待采购"));
        item.complete_when_requirement_satisfied(Instant::from_unix_secs(120))
            .unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(item.completed_by.as_deref(), Some("__system__"));
        assert_eq!(item.owner_user_id.as_deref(), Some("alice"));
        assert!(item
            .complete_when_requirement_satisfied(Instant::from_unix_secs(130))
            .is_err());

        let successor = item
            .successor_for_released_requirement(
                WorkItemId::new("wi-procurement-released"),
                "sales-revision-2".to_string(),
                Some("剩余 4 件待采购".to_string()),
            )
            .unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert_eq!(successor.base.id, "wi-procurement-released");
        assert_eq!(successor.status, WorkItemStatus::Open);
        assert_eq!(successor.subject_version, "sales-revision-2");
        assert_eq!(successor.owner_user_id.as_deref(), Some("alice"));
        assert_eq!(successor.assignment_source, AssignmentSource::SystemRule);
        assert_eq!(successor.responsibility_key(), Some("sales-lines:key"));
        assert_eq!(
            successor.responsibility_scope_ids(),
            item.responsibility_scope_ids()
        );
        assert_eq!(
            successor.reason_code.as_deref(),
            Some("PROCUREMENT_QUANTITY_RELEASED")
        );
        assert_eq!(successor.impact_summary.as_deref(), Some("剩余 4 件待采购"));
    }

    #[test]
    fn responsibility_scope_rejects_empty_lines() {
        assert!(WorkItem::new_with_responsibility_scope(
            WorkItemId::new("wi-empty"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                ..direct_data()
            },
            "sales-lines:key",
            Vec::new(),
        )
        .is_err());
    }

    #[test]
    fn codes_and_bson_shape_are_stable() {
        assert_eq!(AssignmentSource::SystemRule.as_str(), "SYSTEM_RULE");
        assert_eq!(WorkItemType::DocumentApproval.as_str(), "DOCUMENT_APPROVAL");
        let item = WorkItem::new_at(
            WorkItemId::new("wi-1"),
            direct_data(),
            Instant::from_unix_secs(100),
        )
        .unwrap();
        let document = bson::serialize_to_document(&item).unwrap();
        assert_eq!(document.get_str("status").unwrap(), "OPEN");
        let roundtrip: WorkItem = bson::deserialize_from_document(document).unwrap();
        assert_eq!(roundtrip, item);
    }

    /// 单据审批任务固定带个人责任与节点执行，且禁止通用改派。
    ///
    /// 审批运行时完成后任务进入不可逆终态。
    #[test]
    fn document_approval_requires_owner_and_execution() {
        let mut item = approval_item("wi-approval");
        assert_eq!(item.work_item_type, WorkItemType::DocumentApproval);
        assert!(item.reassign("bob", Instant::from_unix_secs(110)).is_err());
        item.complete_by_approval_runtime("alice", Instant::from_unix_secs(110))
            .unwrap();
        assert_eq!(item.status, WorkItemStatus::Completed);
        assert!(ensure_transition(WorkItemStatus::Open, WorkItemStatus::Completed).is_ok());
    }

    /// 决定前置规则直接返回执行引用，并区分责任、状态、版本与引用失败。
    #[test]
    fn approval_decision_preconditions_are_owned_by_work_item() {
        let item = approval_item("wi-decision");
        let version = item.base.version;
        assert_eq!(
            item.approval_execution_for_decision("alice", version)
                .unwrap()
                .as_ref(),
            "exec-1"
        );
        assert_eq!(
            item.approval_execution_for_decision("bob", version),
            Err(ApprovalDecisionTaskError::NotCurrentOwner)
        );
        assert_eq!(
            item.approval_execution_for_decision("alice", version + 1),
            Err(ApprovalDecisionTaskError::VersionConflict)
        );

        let mut closed = item.clone();
        closed
            .close_by_approval_runtime(
                "alice",
                super::WorkItemCloseData {
                    close_reason: "运行时关闭".to_string(),
                },
                Instant::from_unix_secs(110),
            )
            .unwrap();
        assert_eq!(
            closed.approval_execution_for_decision("alice", version),
            Err(ApprovalDecisionTaskError::NotOpen)
        );

        let mut missing = item;
        missing.approval_node_execution_id = None;
        assert_eq!(
            missing.approval_execution_for_decision("alice", version),
            Err(ApprovalDecisionTaskError::MissingExecution)
        );
    }

    /// 审批取消关闭任务时保留 CAS 版本并写入受控关闭事实。
    ///
    /// 批量入口保持输入顺序，受阻取消的空任务集合也是合法结果。
    #[test]
    fn approval_cancellation_closes_open_tasks_without_advancing_version() {
        let item = approval_item("wi-cancel");
        let expected_version = item.base.version;
        let closed = WorkItem::close_all_for_approval_cancellation(
            vec![item],
            "submitter",
            "撤回重改",
            Instant::from_unix_secs(120),
        )
        .unwrap();

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].base.version, expected_version);
        assert_eq!(closed[0].status, WorkItemStatus::Closed);
        assert_eq!(closed[0].closed_by.as_deref(), Some("submitter"));
        assert_eq!(closed[0].close_reason.as_deref(), Some("撤回重改"));
        assert_eq!(
            closed[0].approval_node_execution_id.as_ref().map(AsRef::as_ref),
            Some("exec-1")
        );
        assert!(WorkItem::close_all_for_approval_cancellation(
            Vec::new(),
            "submitter",
            "受阻取消",
            Instant::from_unix_secs(121),
        )
        .unwrap()
        .is_empty());
    }

    /// 审批取消不得关闭独立任务、缺失执行引用或已终态任务。
    ///
    /// 任一非法任务都会使批量规则整体失败关闭。
    #[test]
    fn approval_cancellation_rejects_invalid_task_facts() {
        let generic = WorkItem::new_at(
            WorkItemId::new("wi-generic"),
            direct_data(),
            Instant::from_unix_secs(100),
        )
        .unwrap();
        assert!(generic
            .close_for_approval_cancellation("submitter", "撤回", Instant::from_unix_secs(120))
            .is_err());

        let mut missing_execution = approval_item("wi-missing-execution");
        missing_execution.approval_node_execution_id = None;
        assert!(missing_execution
            .close_for_approval_cancellation("submitter", "撤回", Instant::from_unix_secs(120))
            .is_err());

        let completed = approval_item("wi-completed");
        let mut completed = completed;
        completed
            .complete_by_approval_runtime("alice", Instant::from_unix_secs(110))
            .unwrap();
        assert!(completed
            .close_for_approval_cancellation("submitter", "撤回", Instant::from_unix_secs(120))
            .is_err());

        assert!(WorkItem::close_all_for_approval_cancellation(
            vec![approval_item("wi-valid"), approval_item("wi-invalid")],
            "",
            "撤回",
            Instant::from_unix_secs(120),
        )
        .is_err());
    }
}
