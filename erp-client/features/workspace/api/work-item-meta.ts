/**
 * W01 今日工作台 — 任务类型的展示元数据（类型标签、家族分组、优先级与状态映射）。
 * 与请求编排 dashboard.ts 分离：这些表是纯声明式配置，不参与任何 I/O，
 * 单独存放便于按「一张表一个关注点」维护，避免撑大请求模块。
 */

import type { WorkspaceFamilyFilter, WorkspaceWorkItem } from "../types"

/** 单据类型徽章色。不用 warning / destructive，避免和受阻、超期状态抢语义。 */
export type WorkspaceDocumentBadgeVariant =
    | "info"
    | "success"
    | "orange"
    | "teal"
    | "violet"
    | "lime"
    | "rose"
    | "indigo"
    | "cyan"
    | "neutral"

export type WorkspaceDocumentBadgeMeta = Readonly<{
    label: string
    variant: WorkspaceDocumentBadgeVariant
}>

export const FAMILY_META: Record<
    WorkspaceFamilyFilter,
    { label: string; defaultExpanded: boolean }
> = {
    approval: { label: "审批与确认", defaultExpanded: true },
    procurement: { label: "供给与采购", defaultExpanded: true },
    finance: { label: "票款与结算", defaultExpanded: true },
    fulfillment: { label: "履约与库存", defaultExpanded: false },
    exception: { label: "数据治理与异常", defaultExpanded: false },
}

type TypeMeta = Readonly<{
    label: string
    family: WorkspaceFamilyFilter
    badgeLabel: string
    badgeVariant: WorkspaceDocumentBadgeVariant
    /** 跳转目标工作面的按钮文案；缺省则按单据名生成「打开销售单」。 */
    openActionLabel?: string
}>

export const TYPE_META: Record<string, TypeMeta> = {
    DOCUMENT_APPROVAL: {
        label: "单据审批",
        family: "approval",
        badgeLabel: "单据",
        badgeVariant: "info",
    },
    PROCUREMENT_ORDER_CREATION: {
        label: "待供给分配",
        family: "procurement",
        badgeLabel: "供给分配",
        badgeVariant: "lime",
        openActionLabel: "去分配供给",
    },
    FULFILLMENT_OPERATION: {
        label: "履约处理",
        family: "fulfillment",
        badgeLabel: "收发履约",
        badgeVariant: "teal",
    },
    CUSTOMER_ACCEPTANCE_REGISTRATION: {
        label: "客户验收登记",
        family: "fulfillment",
        badgeLabel: "待客户验收",
        badgeVariant: "success",
        openActionLabel: "去登记客户验收",
    },
    SUPPLIER_PAYMENT_EXECUTION: {
        label: "供应商付款处理",
        family: "finance",
        badgeLabel: "待付款",
        badgeVariant: "cyan",
        openActionLabel: "去登记付款",
    },
    SALES_INVOICE_EXECUTION: {
        label: "销项开票处理",
        family: "finance",
        badgeLabel: "待开票",
        badgeVariant: "violet",
        openActionLabel: "去登记销项发票",
    },
    PURCHASE_ORDER_REVIEW: {
        label: "采购单财务审核",
        family: "finance",
        badgeLabel: "采购审核",
        badgeVariant: "cyan",
        openActionLabel: "去审核采购单",
    },
    SALES_CHANGE_IMPACT_REVIEW: {
        label: "销售变更履约影响复核",
        family: "fulfillment",
        badgeLabel: "变更履约",
        badgeVariant: "teal",
        openActionLabel: "去复核履约影响",
    },
    SALES_CHANGE_FINANCE_REVIEW: {
        label: "销售变更财务复核",
        family: "finance",
        badgeLabel: "变更财务",
        badgeVariant: "violet",
        openActionLabel: "去复核财务影响",
    },
    CARD_FUNDS_REVIEW: {
        label: "卡券票款复核",
        family: "finance",
        badgeLabel: "卡券票款",
        badgeVariant: "orange",
        openActionLabel: "去复核卡券票款",
    },
    CARD_FUNDS_DELTA_REVIEW: {
        label: "卡券票款差异复核",
        family: "finance",
        badgeLabel: "卡券差异",
        badgeVariant: "lime",
        openActionLabel: "去复核票款差额",
    },
    INVENTORY_ADJUSTMENT_REVIEW: {
        label: "库存调整复核",
        family: "fulfillment",
        badgeLabel: "库存调整",
        badgeVariant: "teal",
        openActionLabel: "去复核库存调整",
    },
    SUPPLIER_SETTLEMENT_REVIEW: {
        label: "供应商结算复核",
        family: "finance",
        badgeLabel: "供应商结算",
        badgeVariant: "cyan",
        openActionLabel: "去复核供应商结算",
    },
    IMPORT_BUSINESS_CONFIRMATION: {
        label: "导入业务确认",
        family: "exception",
        badgeLabel: "导入确认",
        badgeVariant: "orange",
        openActionLabel: "去确认导入范围",
    },
    INTEGRATION_RESULT_UNKNOWN: {
        label: "集成结果未知",
        family: "exception",
        badgeLabel: "集成未知",
        badgeVariant: "violet",
        openActionLabel: "去确认集成结果",
    },
    BUSINESS_EXCEPTION: {
        label: "业务异常",
        family: "exception",
        badgeLabel: "业务异常",
        badgeVariant: "rose",
        openActionLabel: "去处理业务异常",
    },
}

type DocumentMeta = Readonly<{
    approvalLabel: string
    badgeLabel: string
    /** 按钮里的单据全称；缺省用徽章短名。 */
    documentName?: string
    badgeVariant: WorkspaceDocumentBadgeVariant
}>

const DOCUMENT_META: Record<string, DocumentMeta> = {
    sales_order: {
        approvalLabel: "销售单审批",
        badgeLabel: "销售单",
        badgeVariant: "info",
    },
    voucher_sales_order: {
        approvalLabel: "卡券销售单审批",
        badgeLabel: "卡券销售",
        documentName: "卡券销售单",
        badgeVariant: "violet",
    },
    sales_change_order: {
        approvalLabel: "销售变更单审批",
        badgeLabel: "销售变更",
        badgeVariant: "indigo",
    },
    purchase_order: {
        approvalLabel: "采购单审批",
        badgeLabel: "采购单",
        badgeVariant: "orange",
    },
    purchase_change_order: {
        approvalLabel: "采购变更单审批",
        badgeLabel: "采购变更",
        badgeVariant: "lime",
    },
    stock_adjustment: {
        approvalLabel: "库存调整单审批",
        badgeLabel: "库存调整",
        documentName: "库存调整单",
        badgeVariant: "teal",
    },
    customer_receipt: {
        approvalLabel: "回款复核",
        badgeLabel: "回款",
        documentName: "回款单",
        badgeVariant: "success",
    },
    supplier_payment: {
        approvalLabel: "付款记录",
        badgeLabel: "付款",
        badgeVariant: "cyan",
    },
    payable_account: {
        approvalLabel: "供应商付款处理",
        badgeLabel: "待付款",
        documentName: "应付账户",
        badgeVariant: "cyan",
    },
    receivable_account: {
        approvalLabel: "销项开票处理",
        badgeLabel: "待开票",
        documentName: "应收账户",
        badgeVariant: "violet",
    },
    customer_refund: {
        approvalLabel: "客户退款审批",
        badgeLabel: "客户退款",
        badgeVariant: "rose",
    },
    supplier_refund: {
        approvalLabel: "供应商退款审批",
        badgeLabel: "供应商退款",
        badgeVariant: "violet",
    },
    receipt_reversal: {
        approvalLabel: "回款冲正审批",
        badgeLabel: "回款冲正",
        badgeVariant: "indigo",
    },
    payment_reversal: {
        approvalLabel: "付款冲正审批",
        badgeLabel: "付款冲正",
        badgeVariant: "lime",
    },
    purchase_receipt: {
        approvalLabel: "采购收货单审批",
        badgeLabel: "采购收货",
        badgeVariant: "orange",
    },
    delivery: {
        approvalLabel: "仓发单审批",
        badgeLabel: "仓发",
        documentName: "仓发单",
        badgeVariant: "teal",
    },
    electronic_delivery: {
        approvalLabel: "电子交付单审批",
        badgeLabel: "电子交付",
        documentName: "电子交付单",
        badgeVariant: "cyan",
    },
    service_fulfillment: {
        approvalLabel: "服务履约单审批",
        badgeLabel: "服务履约",
        documentName: "服务履约单",
        badgeVariant: "success",
    },
    customer_acceptance: {
        approvalLabel: "客户验收单审批",
        badgeLabel: "客户验收",
        documentName: "客户验收单",
        badgeVariant: "info",
    },
    invoice: {
        approvalLabel: "发票审批",
        badgeLabel: "发票",
        badgeVariant: "rose",
    },
    sales_return_case: {
        approvalLabel: "销售退货单审批",
        badgeLabel: "销售退货",
        badgeVariant: "violet",
    },
    purchase_return_order: {
        approvalLabel: "采购退货单审批",
        badgeLabel: "采购退货",
        badgeVariant: "orange",
    },
}

const DOCUMENT_TYPED_WORK_ITEMS = new Set([
    "DOCUMENT_APPROVAL",
    "APPROVAL_INSTANCE",
])

/**
 * 把业务对象种类收成小写稳定码，兼容 PascalCase 与 snake_case。
 *
 * @param businessObjectType 业务对象种类。
 * @returns 例如 `SalesOrder` → `sales_order`。
 */
function normalizeObjectType(businessObjectType: string): string {
    return businessObjectType
        .trim()
        .replace(/([a-z])([A-Z])/g, "$1_$2")
        .toLowerCase()
}

/**
 * 按业务对象种类查找单据展示元数据。
 *
 * @param businessObjectType 业务对象种类，允许 PascalCase 或 snake_case。
 * @returns 命中时返回单据文案与徽章色，否则 undefined。
 */
function documentMetaOf(businessObjectType: string): DocumentMeta | undefined {
    const exact = DOCUMENT_META[businessObjectType]
    if (exact) return exact
    return DOCUMENT_META[normalizeObjectType(businessObjectType)]
}

/**
 * 工作台列表用的任务类型文案。通用单据审批按对象种类细分，避免全部显示 DOCUMENT_APPROVAL。
 *
 * @param workItemType 任务类型码。
 * @param businessObjectType 业务对象种类。
 * @returns 类型标签，例如「销售单审批」。
 */
export function workspaceTypeLabel(
    workItemType: string,
    businessObjectType: string,
): string {
    if (DOCUMENT_TYPED_WORK_ITEMS.has(workItemType)) {
        return (
            documentMetaOf(businessObjectType)?.approvalLabel ??
            TYPE_META.DOCUMENT_APPROVAL.label
        )
    }
    return TYPE_META[workItemType]?.label ?? workItemType
}

/**
 * 工作台单据类型徽章。通用审批按单据种类着色，其它任务按任务类型着色。
 *
 * @param workItemType 任务类型码。
 * @param businessObjectType 业务对象种类。
 * @param fallbackLabel 未知类型时的兜底文案，通常是已算好的 `workItemTypeLabel`。
 * @returns 短标签与徽章色，供队列和详情扫读。
 */
export function workspaceDocumentBadge(
    workItemType: string,
    businessObjectType: string,
    fallbackLabel?: string,
): WorkspaceDocumentBadgeMeta {
    if (DOCUMENT_TYPED_WORK_ITEMS.has(workItemType)) {
        const documentMeta = documentMetaOf(businessObjectType)
        if (documentMeta) {
            return {
                label: documentMeta.badgeLabel,
                variant: documentMeta.badgeVariant,
            }
        }
    }
    const typeMeta = TYPE_META[workItemType]
    if (typeMeta) {
        return {
            label: typeMeta.badgeLabel,
            variant: typeMeta.badgeVariant,
        }
    }
    const label =
        fallbackLabel?.trim() ||
        workspaceTypeLabel(workItemType, businessObjectType)
    return { label, variant: "neutral" }
}

/**
 * 按钮里使用的单据全称，例如「销售单」「卡券销售单」。
 */
export function workspaceDocumentName(
    businessObjectType: string,
): string | undefined {
    const meta = documentMetaOf(businessObjectType)
    if (!meta) return undefined
    return meta.documentName ?? meta.badgeLabel
}

/**
 * 页内纸质预览按钮。按单据命名，如「查看销售单」。
 */
export function workspaceReadActionLabel(businessObjectType: string): string {
    const name = workspaceDocumentName(businessObjectType)
    return name ? `查看${name}` : "查看单据"
}

/**
 * 跳转目标工作面的按钮。专项任务使用对应业务动作；通用审批使用「打开销售单」。
 */
export function workspaceOpenActionLabel(
    workItemType: string,
    businessObjectType: string,
): string {
    const taskLabel = TYPE_META[workItemType]?.openActionLabel
    if (taskLabel) return taskLabel
    const name = workspaceDocumentName(businessObjectType)
    return name ? `打开${name}` : "打开单据"
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
