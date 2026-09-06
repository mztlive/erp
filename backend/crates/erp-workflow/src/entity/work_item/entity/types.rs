//! 工作项类型、状态、简报关系与责任来源。

use serde::{Deserialize, Serialize};

use erp_core::common::state::DocumentState;

/// 当前代码注册的任务类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemType {
    /// 销售单生效后的供给分配（现有库存优先，缺口采购）。
    ProcurementOrderCreation,
    /// 入库、仓发、供应商直发、电子交付或线下服务的具体履约操作。
    FulfillmentOperation,
    /// 发货或交付完成后，由销售登记客户验收结果。
    CustomerAcceptanceRegistration,
    /// 已确认应付的供应商付款执行。
    SupplierPaymentExecution,
    /// 应收子账尚有可开票额度时的销项开票执行。
    SalesInvoiceExecution,
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
    /// 采购入库单。
    PurchaseReceipt,
    /// 发货单。
    Delivery,
    /// 电子交付记录。
    ElectronicDelivery,
    /// 线下服务履约记录。
    ServiceFulfillment,
    /// 采购变更单。
    PurchaseChangeOrder,
    /// 销售变更单。
    SalesChangeOrder,
    /// 应收子账。
    ReceivableAccount,
    /// 应付子账。
    PayableAccount,
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

const WORK_ITEM_BRIEF_RELATIONS: &[WorkItemBriefRelation] = &[
    WorkItemBriefRelation {
        work_item_type: WorkItemType::ProcurementOrderCreation,
        object_kind: WorkItemBriefObjectKind::SalesOrder,
        business_object_type: "sales_order",
        read_permission: "purchase_order:create",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::FulfillmentOperation,
        object_kind: WorkItemBriefObjectKind::PurchaseReceipt,
        business_object_type: "purchase_receipt",
        read_permission: "purchase_receipt:post",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::FulfillmentOperation,
        object_kind: WorkItemBriefObjectKind::Delivery,
        business_object_type: "delivery",
        read_permission: "delivery:post",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::FulfillmentOperation,
        object_kind: WorkItemBriefObjectKind::ElectronicDelivery,
        business_object_type: "electronic_delivery",
        read_permission: "electronic_delivery:confirm",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::FulfillmentOperation,
        object_kind: WorkItemBriefObjectKind::ServiceFulfillment,
        business_object_type: "service_fulfillment",
        read_permission: "service_fulfillment:confirm",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::CustomerAcceptanceRegistration,
        object_kind: WorkItemBriefObjectKind::SalesOrder,
        business_object_type: "sales_order",
        read_permission: "sales_order:detail",
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
        work_item_type: WorkItemType::SupplierPaymentExecution,
        object_kind: WorkItemBriefObjectKind::PayableAccount,
        business_object_type: "payable_account",
        read_permission: "payable_account:detail",
    },
    WorkItemBriefRelation {
        work_item_type: WorkItemType::SalesInvoiceExecution,
        object_kind: WorkItemBriefObjectKind::ReceivableAccount,
        business_object_type: "receivable_account",
        read_permission: "receivable_account:detail",
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
            Self::ProcurementOrderCreation => "供给分配",
            Self::FulfillmentOperation => "履约处理",
            Self::CustomerAcceptanceRegistration => "客户验收登记",
            Self::SupplierPaymentExecution => "供应商付款处理",
            Self::SalesInvoiceExecution => "销项开票处理",
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
            Self::FulfillmentOperation => "FULFILLMENT_OPERATION",
            Self::CustomerAcceptanceRegistration => "CUSTOMER_ACCEPTANCE_REGISTRATION",
            Self::SupplierPaymentExecution => "SUPPLIER_PAYMENT_EXECUTION",
            Self::SalesInvoiceExecution => "SALES_INVOICE_EXECUTION",
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
            Self::FulfillmentOperation | Self::CustomerAcceptanceRegistration => {
                WorkItemAssignmentSeparationPolicy::RoleAndParticipation
            }
            Self::SupplierPaymentExecution | Self::SalesInvoiceExecution => {
                WorkItemAssignmentSeparationPolicy::RoleAndParticipation
            }
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
    /// 供给分配任务返回 `true`。
    pub fn is_procurement_order_creation(self) -> bool {
        self == Self::ProcurementOrderCreation
    }

    /// 判断任务是否为具体履约操作。
    ///
    /// # 返回
    /// 入库、发货、电子交付或服务履约任务返回 `true`。
    pub fn is_fulfillment_operation(self) -> bool {
        self == Self::FulfillmentOperation
    }

    /// 判断任务是否为销售客户验收登记。
    ///
    /// # 返回
    /// W06 客户验收登记任务返回 `true`。
    pub fn is_customer_acceptance_registration(self) -> bool {
        self == Self::CustomerAcceptanceRegistration
    }

    /// 判断任务是否为供应商付款执行。
    ///
    /// # 返回
    /// 已确认应付的付款执行任务返回 `true`。
    pub fn is_supplier_payment_execution(self) -> bool {
        self == Self::SupplierPaymentExecution
    }

    /// 判断任务是否为销项开票执行。
    ///
    /// # 返回
    /// 应收子账开票执行任务返回 `true`。
    pub fn is_sales_invoice_execution(self) -> bool {
        self == Self::SalesInvoiceExecution
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

#[cfg(test)]
mod tests {
    use super::{WorkItemAssignmentSeparationPolicy, WorkItemBriefObjectKind, WorkItemType};

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
        let payable = WorkItemType::SupplierPaymentExecution
            .brief_relation("payable_account")
            .unwrap();
        assert_eq!(payable.object_kind, WorkItemBriefObjectKind::PayableAccount);
        assert_eq!(payable.read_permission, "payable_account:detail");
        let acceptance = WorkItemType::CustomerAcceptanceRegistration
            .brief_relation("sales_order")
            .unwrap();
        assert_eq!(acceptance.object_kind, WorkItemBriefObjectKind::SalesOrder);
        assert_eq!(acceptance.read_permission, "sales_order:detail");
        assert_eq!(
            WorkItemType::CustomerAcceptanceRegistration.assignment_separation_policy(),
            WorkItemAssignmentSeparationPolicy::RoleAndParticipation
        );
    }
}
