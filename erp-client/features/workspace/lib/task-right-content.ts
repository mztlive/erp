import {
    compareDecimal,
    compactFixed,
    formatCurrencyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { StatusTone } from "@/components/ui/status-badge"
import type { WorkspaceWorkItem } from "../types"
import { workspaceTaskSection } from "./task-presentation"
import { isBlockedWorkItem } from "./work-item"

type Metric = Readonly<{
    label: string
    value: string
    detail?: string
    unavailable?: boolean
}>
type Policy = Readonly<{
    metric: string
    fields: readonly string[]
    status: string
    dates?: readonly string[]
    states?: readonly string[]
}>

const POLICIES: Record<string, Policy> = {
    PROCUREMENT_ORDER_CREATION: {
        metric: "待分配范围",
        fields: ["待分配行数", "未覆盖数量"],
        status: "待分配",
        dates: ["需求交期"],
    },
    PURCHASE_SOURCE_UPDATE: {
        metric: "变更影响",
        fields: ["受影响行数", "待处理事项数"],
        status: "待核对",
        states: ["处理阶段"],
    },
    CUSTOMER_ACCEPTANCE_REGISTRATION: {
        metric: "待验收范围",
        fields: ["待验收数量", "待验收行数", "待验收批次"],
        status: "待验收",
        dates: ["验收期限"],
        states: ["验收进度"],
    },
    SUPPLIER_PAYMENT_EXECUTION: {
        metric: "本次待付金额",
        fields: ["待付金额", "未付金额"],
        status: "待付款",
        dates: ["计划付款日", "希望付款日"],
    },
    SALES_INVOICE_EXECUTION: {
        metric: "待开票金额",
        fields: ["待开票金额"],
        status: "待开票",
        dates: ["开票期限"],
    },
    PURCHASE_RETURN_REFUND: {
        metric: "待收退款金额",
        fields: ["待收退款金额", "待退款金额"],
        status: "待处理退款",
        dates: ["约定退款日"],
        states: ["退款状态"],
    },
    PURCHASE_RETURN_INVOICE: {
        metric: "待处理红票金额",
        fields: ["待红冲金额", "待处理红票金额"],
        status: "待处理红票",
        states: ["红票状态"],
    },
    SUPPLIER_SETTLEMENT_REVIEW: {
        metric: "待复核差异",
        fields: ["未解决差异数", "未决差异数", "差异处理"],
        status: "待复核",
        states: ["结算状态"],
    },
    SUPPLIER_PORTAL_REVIEW: {
        metric: "申请内容",
        fields: ["申请类型"],
        status: "待确认",
    },
    IMPORT_BUSINESS_CONFIRMATION: {
        metric: "待确认范围",
        fields: ["待确认行数", "待确认范围"],
        status: "待确认",
    },
    INTEGRATION_RESULT_UNKNOWN: {
        metric: "待核实事项",
        fields: ["错误分类"],
        status: "待核实",
    },
    BUSINESS_EXCEPTION: {
        metric: "异常类型",
        fields: ["差异类型", "错误分类"],
        status: "待处理",
    },
}

const FULFILLMENT: Record<string, Policy> = {
    delivery: {
        metric: "本批待发",
        fields: ["待发数量"],
        status: "待发货",
        dates: ["最早预计交付日", "最早客户交付日"],
    },
    purchase_receipt: {
        metric: "本次待收",
        fields: ["待收数量"],
        status: "待收货",
        dates: ["预计到货日"],
    },
    electronic_delivery: {
        metric: "待交付范围",
        fields: ["待交付数量"],
        status: "待交付",
        dates: ["约定交付日"],
    },
    service_fulfillment: {
        metric: "待履约范围",
        fields: ["待完成项目数", "待履约数量"],
        status: "待履约",
        dates: ["约定完成日"],
    },
}

function objectKind(item: WorkspaceWorkItem) {
    return item.businessObjectType
        .trim()
        .replace(/([a-z])([A-Z])/g, "$1_$2")
        .toLowerCase()
}

function firstSection(item: WorkspaceWorkItem, labels: readonly string[]) {
    for (const label of labels) {
        const field = workspaceTaskSection(item, label)
        if (field) return field
    }
    return undefined
}

function unavailable(label: string): Metric {
    return {
        label,
        value: "查看明细",
        detail: "列表未提供具体数值，进入事项核对。",
        unavailable: true,
    }
}

/** 仅汇总完整发货批次已返回的待发数量；不同或缺失单位时用待发行数。 */
function deliveryMetric(item: WorkspaceWorkItem): Metric | undefined {
    const lines = item.deliveryLines
    if (!lines?.length) return undefined
    try {
        const pending = lines.filter(
            (line) => compareDecimal(line.pending_quantity, "0", 6) > 0,
        )
        const unit = lines[0].unit?.trim()
        if (unit && lines.every((line) => line.unit?.trim() === unit)) {
            const value = compactFixed(
                sumFixed(
                    lines.map((line) => line.pending_quantity),
                    { maxScale: 6, outputScale: 6 },
                ),
            )
            return {
                label: "本批待发",
                value: `${value} ${unit}`,
                detail: `本批共 ${lines.length} 行`,
            }
        }
        return {
            label: "本批待发行数",
            value: `${pending.length} 行`,
            detail: "各商品数量及单位见明细",
        }
    } catch {
        return unavailable("本批待发")
    }
}

/** 精确读取服务端金额对照格式；不把数量、价格混合摘要当成金额。 */
function changeAmount(value: string): Metric | undefined {
    const match = value.match(
        /^\s*[¥￥]([\d,]+(?:\.\d+)?)\s*→\s*[¥￥]([\d,]+(?:\.\d+)?)\s*$/,
    )
    if (!match) return undefined
    try {
        const before = match[1].replaceAll(",", "")
        const after = match[2].replaceAll(",", "")
        const direction = compareDecimal(after, before, 6)
        if (direction === 0) return undefined
        const difference =
            direction > 0
                ? subtractFixed(after, before, { maxScale: 6, outputScale: 6 })
                : subtractFixed(before, after, { maxScale: 6, outputScale: 6 })
        return {
            label: "本次金额变化",
            value: `${direction > 0 ? "增加" : "减少"} ${formatCurrencyFixed(difference, { maxScale: 6, maximumFractionDigits: 6 })}`,
            detail: value,
        }
    } catch {
        return undefined
    }
}

function changeMetric(item: WorkspaceWorkItem): Metric {
    const amount = workspaceTaskSection(item, "含税金额（前→后）")
    const amountChange = amount && changeAmount(amount.value)
    if (amountChange) return amountChange
    const dateChange = item.briefLines?.find((line) =>
        line.dueLabel?.includes("→"),
    )?.dueLabel
    if (dateChange)
        return { label: "本次交期变化", value: "交期调整", detail: dateChange }
    const quantityChange = item.briefLines?.find((line) =>
        line.quantity?.includes("→"),
    )?.quantity
    if (quantityChange)
        return {
            label: "本次明细变化",
            value:
                quantityChange.includes("数量") &&
                !quantityChange.includes("价")
                    ? "数量调整"
                    : quantityChange.includes("价") &&
                        !quantityChange.includes("数量")
                      ? "价格调整"
                      : "明细调整",
            detail: quantityChange,
        }
    const field = firstSection(item, ["明细差异", "变更类型"])
    return field
        ? { label: "本次变更", value: field.value }
        : unavailable("本次变更")
}

/** 审批明细数量不从截断行估算；总行数仅在服务端明确返回剩余行数时展示。 */
function approvalMetric(
    item: WorkspaceWorkItem,
    kind: string,
    amount?: Metric,
): Metric {
    if (kind === "sales_change_order" || kind === "purchase_change_order")
        return changeMetric(item)
    if (
        [
            "stock_adjustment",
            "purchase_receipt",
            "delivery",
            "electronic_delivery",
            "service_fulfillment",
            "customer_acceptance",
            "sales_return_case",
            "purchase_return_order",
        ].includes(kind)
    ) {
        const field = firstSection(item, [
            "本次数量",
            "调整数量",
            "本次提交数量",
            "明细",
        ])
        if (field) return { label: field.label, value: field.value }
        if (item.briefLines?.length && item.briefMoreCount != null) {
            return {
                label: "本次提交明细",
                value: `${item.briefLines.length + item.briefMoreCount} 行`,
                detail:
                    kind === "stock_adjustment" && amount
                        ? `${amount.label} ${amount.value}`
                        : undefined,
            }
        }
        return unavailable("本次提交范围")
    }
    return amount || unavailable("本次申请金额")
}

function policyFor(item: WorkspaceWorkItem) {
    return item.workItemType === "FULFILLMENT_OPERATION"
        ? FULFILLMENT[objectKind(item)]
        : POLICIES[item.workItemType]
}

/** 各任务选取本次工作量，缺失时保留未知，不借用整单金额或销售数量。 */
function workMetric(item: WorkspaceWorkItem, amount?: Metric): Metric {
    const kind = objectKind(item)
    if (["DOCUMENT_APPROVAL", "APPROVAL_INSTANCE"].includes(item.workItemType))
        return approvalMetric(item, kind, amount)
    if (item.workItemType === "FULFILLMENT_OPERATION" && kind === "delivery") {
        const metric = deliveryMetric(item)
        if (metric) return metric
    }
    const policy = policyFor(item)
    if (!policy) return unavailable("本次处理")
    const field = firstSection(item, policy.fields)
    return field
        ? { label: field.label, value: field.value }
        : unavailable(policy.metric)
}

function dateInfo(
    item: WorkspaceWorkItem,
    policy?: Policy,
): { label: string; value: string; dateTime?: string } | undefined {
    if (item.dueAt) {
        const date = new Date(item.dueAt)
        if (!Number.isNaN(date.getTime()))
            return {
                label:
                    item.workItemType === "DOCUMENT_APPROVAL"
                        ? "审批截止"
                        : "任务截止",
                value: new Intl.DateTimeFormat("zh-CN", {
                    month: "numeric",
                    day: "numeric",
                    hour: "2-digit",
                    minute: "2-digit",
                    hour12: false,
                }).format(date),
                dateTime: item.dueAt,
            }
    }
    const field = firstSection(item, policy?.dates ?? [])
    return field ? { label: field.label, value: field.value } : undefined
}

export function workspaceTaskRightContent(
    item: WorkspaceWorkItem,
    amount?: Metric,
    approvalProgress?: string,
) {
    const tracking = item.workItemType === "APPROVAL_INSTANCE"
    const policy = policyFor(item)
    const blocked = isBlockedWorkItem(item)
    const label = tracking
        ? item.statusLabel
        : blocked
          ? "受阻"
          : item.dueBucket === "overdue"
            ? "已超期"
            : item.dueBucket === "today"
              ? "今日到期"
              : firstSection(item, policy?.states ?? [])?.value ||
                (item.workItemType === "DOCUMENT_APPROVAL"
                    ? "待审批"
                    : policy?.status || "待处理")
    const tone: StatusTone = tracking
        ? item.statusTone
        : blocked || item.dueBucket === "today"
          ? "warning"
          : item.dueBucket === "overdue"
            ? "destructive"
            : "neutral"
    return {
        metric: workMetric(item, amount),
        status: { label, tone, detail: approvalProgress },
        date: tracking ? undefined : dateInfo(item, policy),
        tracking,
    }
}

/** 表头依据用户选择的分类，而非当前页碰巧只有一种任务。 */
export function workspaceTaskColumnLabels(
    workItemType?: string,
    tracking = false,
) {
    if (tracking) return { metric: "申请摘要", status: "审批进度" }
    if (workItemType === "DOCUMENT_APPROVAL")
        return { metric: "审批内容", status: "审批状态" }
    if (workItemType === "FULFILLMENT_OPERATION")
        return { metric: "本次履约", status: "履约状态" }
    return {
        metric: workItemType
            ? POLICIES[workItemType]?.metric || "本次处理"
            : "本次处理",
        status: "处理状态",
    }
}
