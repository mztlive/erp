import { formatDateTime } from "@/lib/datetime"
import { formatCurrencyFixed, formatFixedDisplay } from "@/lib/fixed-decimal"
import type { AuditLogItem, AuditValue, BusinessAuditResult } from "../types"

export const BUSINESS_AUDIT_ACTIONS = [
    { value: "service_fulfillment.confirm", label: "确认服务履约" },
    { value: "purchase_invoice_allocation.post", label: "登记进项发票" },
] as const

export const BUSINESS_AUDIT_RESULTS = [
    { value: "succeeded", label: "执行成功" },
    { value: "rejected", label: "已拒绝" },
    { value: "unknown", label: "结果待确认" },
] as const

export function auditValueLabel(value: AuditValue): string {
    switch (value.kind) {
        case "code":
            return value.label
        case "changed":
            return "已变更"
        case "quantity":
            return formatFixedDisplay(value.value, { maxScale: 6 })
        case "amount":
            return formatCurrencyFixed(value.value, {
                maxScale: 2,
                minimumFractionDigits: 2,
            })
    }
}

export function auditActorLabel(row: AuditLogItem): string {
    return row.structured_event?.actor_name_snapshot || "姓名未记录"
}

export function auditActionLabel(row: AuditLogItem): string {
    return row.structured_event?.action_label || "中文动作未记录"
}

export function auditObjectLabel(row: AuditLogItem): string {
    return row.structured_event?.resource_number_snapshot || "业务编号未记录"
}

export function auditTimeLabel(row: AuditLogItem): string {
    const time = row.structured_event?.occurred_at ?? row.created_at
    return formatDateTime(new Date(time * 1000).toISOString(), "full")
}

export function auditResultView(row: AuditLogItem) {
    const result = row.structured_event?.result
    if (!result) {
        return {
            label: row.success ? "原记录成功" : "原记录失败",
            tone: row.success ? ("success" as const) : ("destructive" as const),
        }
    }
    const views = {
        succeeded: { label: "执行成功", tone: "success" as const },
        rejected: { label: "已拒绝", tone: "destructive" as const },
        unknown: { label: "结果待确认", tone: "warning" as const },
    } satisfies Record<BusinessAuditResult, { label: string; tone: string }>
    return views[result]
}

export function auditChangesLabel(row: AuditLogItem): string {
    const event = row.structured_event
    if (!event) return "字段变化未记录"
    if (event.field_changes.length === 0) return "无字段变化"
    return event.field_changes
        .map((change) =>
            change.before.kind === "changed" || change.after.kind === "changed"
                ? `${change.field_label}：已变更`
                : `${change.field_label}：${auditValueLabel(change.before)} → ${auditValueLabel(change.after)}`,
        )
        .join("；")
}
