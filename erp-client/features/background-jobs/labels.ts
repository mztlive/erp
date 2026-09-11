import type { BackgroundJobStatus } from "@/components/business"
import type { JobStatus } from "@/features/background-jobs/api"

export const JOB_STATUS_LABELS: Record<JobStatus, string> = {
    pending: "等待执行",
    running: "执行中",
    partially_succeeded: "部分成功",
    succeeded: "已完成",
    failed: "执行失败",
    cancelled: "已取消",
}

/** 任务类型（后端 `job_type`）中文映射。 */
export const JOB_TYPE_LABELS: Record<string, string> = {
    import: "导入",
    export: "导出",
    batch: "批量",
    sync: "同步",
    backfill: "回填",
    reconciliation: "对账",
}

/** 业务类型（后端 `domain_job_type`）中文映射。 */
export const JOB_DOMAIN_LABELS: Record<string, string> = {
    PRODUCT_IMPORT: "商品导入",
    SUPPLIER_IMPORT: "供应商导入",
    SALES_ORDER_EXPORT: "销售单导出",
    INVENTORY_LEDGER_EXPORT: "库存台账导出",
    supplier_fulfillment_order_export: "供应商订单导出",
    CONTRACT_EXPORT: "合同导出",
}

/** 任务类型筛选项：全部 + 已知任务类型。 */
export const JOB_TYPE_FILTER_OPTIONS: ReadonlyArray<{
    value: string
    label: string
}> = [
    { value: "", label: "全部类型" },
    ...Object.entries(JOB_TYPE_LABELS).map(([value, label]) => ({
        value,
        label,
    })),
]

/** 业务类型筛选项：全部 + 已知业务。 */
export const JOB_DOMAIN_FILTER_OPTIONS: ReadonlyArray<{
    value: string
    label: string
}> = [
    { value: "", label: "全部业务" },
    ...Object.entries(JOB_DOMAIN_LABELS).map(([value, label]) => ({
        value,
        label,
    })),
]

/** 任务标题：优先业务类型，其次任务类型。 */
export function backgroundJobDomainLabel(
    domainJobType: string | null,
    jobType: string | null,
): string {
    if (domainJobType) {
        return JOB_DOMAIN_LABELS[domainJobType] ?? domainJobType
    }
    if (jobType) return JOB_TYPE_LABELS[jobType] ?? "后台任务"
    return "后台任务"
}

export function jobProgressStatus(status: JobStatus): BackgroundJobStatus {
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

/** 已完成的部分成功任务不再轮询或提供取消操作。 */
export function isJobActive(
    status: JobStatus,
    finishedAt: number | null = null,
): boolean {
    if (finishedAt !== null) return false
    return (
        status === "pending" ||
        status === "running" ||
        status === "partially_succeeded"
    )
}

/** 可查看全部后台任务的管理员角色，与服务端判定口径一致。 */
const BACKGROUND_JOB_ADMIN_ROLES = ["role-root", "role-sysadmin"] as const

export function isBackgroundJobAdmin(
    roleIds: readonly string[] | undefined | null,
): boolean {
    if (!roleIds) return false
    return roleIds.some((roleId) =>
        (BACKGROUND_JOB_ADMIN_ROLES as readonly string[]).includes(roleId),
    )
}

export function formatJobDateTime(value: number | null): string {
    if (value === null || !Number.isFinite(value)) return "—"
    return new Date(value * 1000).toLocaleString("zh-CN", { hour12: false })
}
