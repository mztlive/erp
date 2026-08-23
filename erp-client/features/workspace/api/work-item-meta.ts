/**
 * W01 今日工作台 — 任务类型的展示元数据（类型标签、家族分组、优先级与状态映射）。
 * 与请求编排 dashboard.ts 分离：这些表是纯声明式配置，不参与任何 I/O，
 * 单独存放便于按「一张表一个关注点」维护，避免撑大请求模块。
 */

import type { WorkspaceFamilyFilter, WorkspaceWorkItem } from "../types"

export const FAMILY_META: Record<
    WorkspaceFamilyFilter,
    { label: string; defaultExpanded: boolean }
> = {
    approval: { label: "审批与确认", defaultExpanded: true },
    finance: { label: "票款与结算", defaultExpanded: true },
    fulfillment: { label: "履约与库存", defaultExpanded: false },
    exception: { label: "数据治理与异常", defaultExpanded: false },
}

export const TYPE_META: Record<
    string,
    {
        label: string
        family: WorkspaceFamilyFilter
    }
> = {
    DOCUMENT_APPROVAL: {
        label: "单据审批",
        family: "approval",
    },
    PROCUREMENT_CONFIRMATION: {
        label: "采购二次确认",
        family: "fulfillment",
    },
    LOW_MARGIN_MANAGER_CONFIRMATION: {
        label: "低毛利销售审批",
        family: "approval",
    },
    PURCHASE_ORDER_REVIEW: {
        label: "采购单财务审核",
        family: "finance",
    },
    SALES_CHANGE_IMPACT_REVIEW: {
        label: "销售变更履约影响复核",
        family: "fulfillment",
    },
    SALES_CHANGE_FINANCE_REVIEW: {
        label: "销售变更财务复核",
        family: "finance",
    },
    CARD_FUNDS_REVIEW: {
        label: "卡券票款复核",
        family: "finance",
    },
    CARD_FUNDS_DELTA_REVIEW: {
        label: "卡券票款差异复核",
        family: "finance",
    },
    CARD_SALES_MANAGER_APPROVAL: {
        label: "卡券销售领导审批",
        family: "approval",
    },
    CARD_SALES_OPERATION_APPROVAL: {
        label: "卡券运营审批",
        family: "approval",
    },
    OWNERSHIP_MIGRATION_SALES_CONFIRMATION: {
        label: "归属迁移销售确认",
        family: "approval",
    },
    OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION: {
        label: "归属迁移财务确认",
        family: "finance",
    },
    INVENTORY_ADJUSTMENT_REVIEW: {
        label: "库存调整复核",
        family: "fulfillment",
    },
    FINANCE_CORRECTION_REVIEW: {
        label: "财务纠错复核",
        family: "finance",
    },
    SUPPLIER_SETTLEMENT_REVIEW: {
        label: "供应商结算复核",
        family: "finance",
    },
    IMPORT_BUSINESS_CONFIRMATION: {
        label: "导入业务确认",
        family: "exception",
    },
    INTEGRATION_RESULT_UNKNOWN: {
        label: "集成结果未知",
        family: "exception",
    },
    BUSINESS_EXCEPTION: {
        label: "业务异常",
        family: "exception",
    },
}

const DOCUMENT_APPROVAL_LABEL: Record<string, string> = {
    SalesOrder: "销售单审批",
    VoucherSalesOrder: "卡券销售单审批",
    PurchaseOrder: "采购单审批",
    CustomerReceipt: "回款复核",
    CustomerRefund: "客户退款审批",
    ReceiptReversal: "回款冲正审批",
    sales_order: "销售单审批",
    voucher_sales_order: "卡券销售单审批",
    purchase_order: "采购单审批",
    customer_receipt: "回款复核",
    customer_refund: "客户退款审批",
    receipt_reversal: "回款冲正审批",
}

/**
 * 工作台列表用的任务类型文案。通用单据审批按对象种类细分，避免全部显示 DOCUMENT_APPROVAL。
 *
 * @param workItemType 任务类型码。
 * @param businessObjectType 业务对象种类。
 * @returns 列表第一行类型标签。
 */
export function workspaceTypeLabel(
    workItemType: string,
    businessObjectType: string,
): string {
    if (workItemType === "DOCUMENT_APPROVAL") {
        return (
            DOCUMENT_APPROVAL_LABEL[businessObjectType] ??
            TYPE_META.DOCUMENT_APPROVAL.label
        )
    }
    return TYPE_META[workItemType]?.label ?? workItemType
}

export const PRIORITY_RANK: Record<string, number> = {
    urgent: 1,
    high: 2,
    normal: 3,
    low: 4,
}

export const STATUS_LABEL: Record<
    WorkspaceWorkItem["status"],
    { label: string; tone: WorkspaceWorkItem["statusTone"] }
> = {
    OPEN: { label: "待处理", tone: "info" },
    COMPLETED: { label: "已完成", tone: "success" },
    CLOSED: { label: "已关闭", tone: "neutral" },
}
