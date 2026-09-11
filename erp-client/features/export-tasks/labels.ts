import type { BackgroundJobStatus } from "@/components/business"
import type { ExportJobStatus } from "@/features/export-tasks/api"

export const EXPORT_JOB_STATUS_LABELS: Record<ExportJobStatus, string> = {
    pending: "等待执行",
    running: "执行中",
    partially_succeeded: "部分成功",
    succeeded: "已完成",
    failed: "执行失败",
    cancelled: "已取消",
}

export const EXPORT_JOB_DOMAIN_LABELS: Record<string, string> = {
    SALES_ORDER_EXPORT: "销售单导出",
    INVENTORY_LEDGER_EXPORT: "库存台账导出",
    supplier_fulfillment_order_export: "供应商订单导出",
    CONTRACT_EXPORT: "合同导出",
}

export function exportDomainLabel(domainJobType: string | null): string {
    if (!domainJobType) return "通用导出"
    return EXPORT_JOB_DOMAIN_LABELS[domainJobType] ?? domainJobType
}

export function exportProgressStatus(
    status: ExportJobStatus,
): BackgroundJobStatus {
    switch (status) {
        case "running":
            return "running"
        case "succeeded":
            return "succeeded"
        case "partially_succeeded":
            return "partial"
        case "failed":
            return "failed"
        case "cancelled":
            return "frozen"
        default:
            return "queued"
    }
}

export function isExportJobActive(status: ExportJobStatus): boolean {
    return (
        status === "pending" ||
        status === "running" ||
        status === "partially_succeeded"
    )
}

/** 可查看全部后台任务的管理员角色，与服务端判定口径一致。 */
const EXPORT_TASK_ADMIN_ROLES = ["role-root", "role-sysadmin"] as const

export function isExportTaskAdmin(
    roleIds: readonly string[] | undefined | null,
): boolean {
    if (!roleIds) return false
    return roleIds.some((roleId) =>
        (EXPORT_TASK_ADMIN_ROLES as readonly string[]).includes(roleId),
    )
}

export function formatExportDateTime(value: number | null): string {
    if (value === null || !Number.isFinite(value)) return "—"
    return new Date(value * 1000).toLocaleString("zh-CN", { hour12: false })
}
