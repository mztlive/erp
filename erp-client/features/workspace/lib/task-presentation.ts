import { displayName } from "@/lib/display-name"
import { workspaceTypeLabel } from "../api/work-item-meta"
import type { WorkspaceWorkItem } from "../types"
import { stripDocumentNumberPrefix } from "./stable-number"
import { isBlockedWorkItem } from "./work-item"

type SummaryPolicy = Readonly<{
    fields: readonly string[]
    lines?: "goods" | "detail"
    lineFirst?: boolean
}>

const GOODS: SummaryPolicy = { fields: [], lines: "goods", lineFirst: true }
const CHANGE: SummaryPolicy = {
    fields: ["变更类型", "原因"],
    lines: "detail",
    lineFirst: true,
}
const INVOICE: SummaryPolicy = { fields: ["开票内容", "开票类型", "备注"] }
const REFUND: SummaryPolicy = {
    fields: ["退款原因", "原因", "原始资金单据", "核销影响"],
}
const REVERSAL: SummaryPolicy = {
    fields: ["冲正原因", "原因", "原始资金单据", "原付款单", "核销影响"],
}

const TASK_POLICIES: Record<string, SummaryPolicy> = {
    PROCUREMENT_ORDER_CREATION: GOODS,
    PURCHASE_PAYMENT_ARRANGEMENT: {
        fields: ["付款条件", "可申请余额", "下次跟进"],
    },
    PURCHASE_SOURCE_UPDATE: { fields: ["变更类型", "原因", "来源销售单"] },
    FULFILLMENT_OPERATION: {
        fields: ["批次明细", "发货仓库", "供应商", "最早预计交付日"],
        lines: "goods",
        lineFirst: true,
    },
    CUSTOMER_ACCEPTANCE_REGISTRATION: GOODS,
    SUPPLIER_PAYMENT_EXECUTION: { fields: ["申请说明", "采购单", "付款条件"] },
    SALES_INVOICE_EXECUTION: INVOICE,
    PURCHASE_RETURN_REFUND: {
        fields: ["退款状态", "退款原因", "退货单", "处理说明"],
    },
    PURCHASE_RETURN_INVOICE: {
        fields: ["开票内容", "红票状态", "退货单", "处理说明"],
    },
    SUPPLIER_SETTLEMENT_REVIEW: {
        fields: ["差异处理", "结算期间", "复核条件"],
        lines: "detail",
    },
    SUPPLIER_PORTAL_REVIEW: {
        fields: ["申请类型", "商品名称", "申请原因", "影响范围"],
    },
    IMPORT_BUSINESS_CONFIRMATION: {
        fields: ["待确认范围", "导入批次", "错误摘要", "安全下一步"],
    },
    INTEGRATION_RESULT_UNKNOWN: {
        fields: ["错误摘要", "错误分类", "安全下一步"],
    },
    BUSINESS_EXCEPTION: {
        fields: ["错误摘要", "差异类型", "关闭条件", "安全下一步"],
    },
}

const DOCUMENT_POLICIES: Record<string, SummaryPolicy> = {
    sales_order: GOODS,
    voucher_sales_order: GOODS,
    purchase_order: { fields: ["采购类型"], lines: "goods", lineFirst: true },
    sales_change_order: CHANGE,
    purchase_change_order: CHANGE,
    purchase_payment_request: { fields: ["申请说明", "我方付款主体"] },
    sales_invoice_request: INVOICE,
    invoice: INVOICE,
    customer_receipt: { fields: ["到账日", "银行流水"] },
    customer_refund: REFUND,
    supplier_refund: REFUND,
    receipt_reversal: REVERSAL,
    payment_reversal: REVERSAL,
    stock_adjustment: {
        fields: ["调整原因", "说明"],
        lines: "detail",
        lineFirst: true,
    },
    purchase_receipt: GOODS,
    delivery: GOODS,
    electronic_delivery: GOODS,
    service_fulfillment: GOODS,
    customer_acceptance: GOODS,
    sales_return_case: {
        fields: ["退货原因", "原因"],
        lines: "goods",
        lineFirst: true,
    },
    purchase_return_order: {
        fields: ["退货原因", "原因"],
        lines: "goods",
        lineFirst: true,
    },
}

const TASK_AMOUNTS: Record<string, readonly string[]> = {
    SALES_INVOICE_EXECUTION: ["待开票金额"],
    PURCHASE_RETURN_REFUND: ["待退款金额", "退款金额"],
    PURCHASE_RETURN_INVOICE: ["待红冲金额", "红票金额"],
}

const DOCUMENT_AMOUNTS: Record<string, readonly string[]> = {
    purchase_payment_request: ["申请金额"],
    sales_invoice_request: ["申请金额"],
    customer_receipt: ["回款金额"],
    customer_refund: ["退款金额"],
    supplier_refund: ["退款金额"],
    receipt_reversal: ["冲正金额"],
    payment_reversal: ["冲正金额"],
    stock_adjustment: ["调整成本"],
    payable_account: ["未付金额"],
    receivable_account: ["待开票金额"],
}

/** 只从当前列表已授权的字段取值，不补查或替换审批提交资料。 */
export function workspaceTaskSection(item: WorkspaceWorkItem, label: string) {
    return item.summarySections?.find(
        (field) =>
            field.label === label && displayName(field.value, field.objectId),
    )
}

function join(parts: readonly (string | undefined)[]): string | undefined {
    const values = [
        ...new Set(parts.map((part) => part?.trim()).filter(Boolean)),
    ]
    return values.length ? values.join(" · ") : undefined
}

/** 明细标题只去掉明确的空规格占位，不拆解列表长摘要。 */
function goodsTitle(title: string): string {
    return title
        .trim()
        .replace(/(?:\s+|^)无规格(?=\s+·|$)/, "")
        .trim()
}

/** 商品简报 quantity 的既有格式为“数量 单位 · ¥行金额”，仅移除该金额尾段。 */
function goodsQuantity(quantity?: string): string | undefined {
    return (
        quantity?.replace(/(?:^|\s+·\s+)[¥￥][\d,.]+$/, "").trim() || undefined
    )
}

function lineSummary(item: WorkspaceWorkItem, mode: "goods" | "detail") {
    if (
        item.workItemType === "FULFILLMENT_OPERATION" &&
        item.deliveryLines?.length
    ) {
        const [first] = item.deliveryLines
        return join([
            goodsTitle(first.title),
            `本批 ${first.quantity}${first.unit ? ` ${first.unit}` : ""}`,
            `待发 ${first.pending_quantity}${first.unit ? ` ${first.unit}` : ""}`,
            item.deliveryLines.length > 1
                ? `另 ${item.deliveryLines.length - 1} 行`
                : undefined,
        ])
    }
    const lines = item.briefLines ?? []
    const first = lines[0]
    if (!first) return undefined
    const remaining = Math.max(0, lines.length - 1) + (item.briefMoreCount ?? 0)
    return join([
        goodsTitle(first.title),
        mode === "goods" ? goodsQuantity(first.quantity) : first.quantity,
        first.dueLabel,
        remaining > 0 ? `另 ${remaining} 行明细` : undefined,
    ])
}

function attentionSummary(item: WorkspaceWorkItem): string | undefined {
    if (isBlockedWorkItem(item)) {
        return (
            join(item.actionBlockers.map((blocker) => blocker.message)) ||
            workspaceTaskSection(item, "受阻原因")?.value ||
            workspaceTaskSection(item, "错误摘要")?.value ||
            "当前事项受阻，请查看原因。"
        )
    }
    if (
        item.approval?.status === "REJECTED" &&
        item.approval.lastRejectReason
    ) {
        return `退回原因：${item.approval.lastRejectReason}`
    }
    return join(
        item.summarySections
            ?.filter((field) => field.label === "供给影响提示")
            .map((field) => field.value) ?? [],
    )
}

/** 金额保留服务端口径；未提供本次金额时不借用原单、税额或零值。 */
function taskAmount(item: WorkspaceWorkItem, kind: string) {
    if (item.amountSummary) return item.amountSummary
    const labels =
        item.workItemType === "SUPPLIER_PAYMENT_EXECUTION"
            ? kind === "purchase_payment_request"
                ? ["待付金额", "申请金额"]
                : ["未付金额"]
            : (TASK_AMOUNTS[item.workItemType] ??
              DOCUMENT_AMOUNTS[kind] ?? [
                  "含税金额",
                  "含税总额",
                  "来源销售单金额",
                  "来源采购单金额",
                  "ERP 金额",
                  "票款金额",
              ])
    for (const label of labels) {
        const value = workspaceTaskSection(item, label)
        if (value) return value
    }
    return undefined
}

/** 只有审批仍在进行时展示当前节点和处理人，完成记录不沿用旧待办节点。 */
function approvalProgress(item: WorkspaceWorkItem) {
    if (!item.approval || item.status !== "OPEN") return undefined
    return join([
        displayName(item.approval.currentNodeLabel),
        displayName(item.approval.currentAssigneeLabel),
    ])
}

/** 所有任务共用标题、身份、摘要；具体摘要按任务或审批单据选取。 */
export function workspaceTaskPresentation(item: WorkspaceWorkItem) {
    const kind = item.businessObjectType
        .trim()
        .replace(/([a-z])([A-Z])/g, "$1_$2")
        .toLowerCase()
    const approval =
        item.workItemType === "DOCUMENT_APPROVAL" ||
        item.workItemType === "APPROVAL_INSTANCE"
    const policy = approval
        ? DOCUMENT_POLICIES[kind]
        : TASK_POLICIES[item.workItemType]
    const title =
        displayName(item.counterpartyName) ||
        workspaceTaskSection(item, "仓库")?.value ||
        workspaceTaskSection(item, "导入批次")?.value ||
        item.objectTitle
    const fields =
        policy?.fields
            .map((label) => {
                const value = workspaceTaskSection(item, label)?.value
                return value && value !== title
                    ? `${label}：${value}`
                    : undefined
            })
            .filter(Boolean)
            .slice(0, 2) ?? []
    const line = policy?.lines ? lineSummary(item, policy.lines) : undefined
    const summary = join(
        item.workItemType === "PURCHASE_SOURCE_UPDATE"
            ? [item.reasonLabel, ...fields]
            : policy?.lineFirst
              ? [line, ...fields]
              : [...fields, line],
    )
    const attention = attentionSummary(item)
    return {
        title,
        typeLabel: approval
            ? workspaceTypeLabel(item.workItemType, kind)
            : item.workItemTypeLabel,
        number: stripDocumentNumberPrefix(item.stableNumber),
        summary:
            (item.workItemType === "PURCHASE_PAYMENT_ARRANGEMENT"
                ? item.impactSummary
                : undefined) ||
            attention ||
            workspaceTaskSection(item, "历史资料")?.value ||
            summary ||
            (item.family === "exception"
                ? join([item.reasonLabel, item.nextActionHint])
                : undefined) ||
            item.listSummary ||
            undefined,
        attention: Boolean(attention),
        amount: taskAmount(item, kind),
        approvalProgress: approvalProgress(item),
    }
}
