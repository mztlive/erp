import type { BackgroundJobStatus } from "@/components/business"
import type {
    BackgroundJobView,
    JobStatus,
} from "@/features/background-jobs/api"

export const JOB_STATUS_LABELS: Record<JobStatus, string> = {
    pending: "等待执行",
    running: "执行中",
    partially_succeeded: "部分成功",
    succeeded: "执行完成",
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
    DATA_TRANSFER: "导入导出",
    SUPPLIER_IMPORT: "供应商导入",
    SUPPLIER_HEALTH_CHECK: "供应商接口检测",
    SUPPLIER_CATALOG_SYNC: "供应商目录同步",
    LEGACY_IMPORT: "历史期初导入",
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

/** 业务分类：优先业务类型，其次任务类型。 */
export function backgroundJobDomainLabel(
    domainJobType: string | null,
    jobType: string | null,
): string {
    if (domainJobType) {
        return JOB_DOMAIN_LABELS[domainJobType] ?? "其他后台任务"
    }
    if (jobType) return JOB_TYPE_LABELS[jobType] ?? "后台任务"
    return "后台任务"
}

/** 文件任务的 job_no 格式为「业务名称 · DT-编号」，拆分用于列表与预览。 */
export function backgroundJobIdentity(
    job: Pick<BackgroundJobView, "job_no" | "domain_job_type" | "job_type">,
): { title: string; number: string } {
    if (job.domain_job_type === "DATA_TRANSFER") {
        const separator = job.job_no.lastIndexOf(" · DT-")
        if (separator > 0) {
            return {
                title: job.job_no.slice(0, separator),
                number: job.job_no.slice(separator + 3),
            }
        }
    }
    return {
        title: backgroundJobDomainLabel(job.domain_job_type, job.job_type),
        number: job.job_no,
    }
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

/** 文件导入开始后不可取消，其他任务沿用服务端的活动状态约束。 */
export function canCancelJob(job: BackgroundJobView): boolean {
    return (
        isJobActive(job.status, job.finished_at) &&
        !(
            job.domain_job_type === "DATA_TRANSFER" &&
            job.job_type === "import" &&
            job.status !== "pending"
        )
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
