import type { BackgroundJobStatus } from "@/components/business"

export const PRODUCT_IMPORT_STATUS_LABELS = {
    pending: "等待执行",
    running: "导入中",
    partially_succeeded: "部分成功",
    succeeded: "导入完成",
    failed: "导入失败",
    cancelled: "已取消",
} as const

export const PRODUCT_IMPORT_ITEM_LABELS = {
    success: "导入成功",
    skipped: "重复跳过",
    failed: "导入失败",
} as const

export function productImportProgressStatus(
    status: string,
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

export function isProductImportActive(status: string): boolean {
    return status === "pending" || status === "running"
}

/** 进行中的任务始终展示；已结束的只保留本次提交且未关闭的任务。 */
export function visibleProductImportJobs<T extends { id: string; status: string }>(
    jobs: readonly T[],
    sessionJobIds: readonly string[],
): T[] {
    const session = new Set(sessionJobIds)
    return jobs.filter(
        (job) => isProductImportActive(job.status) || session.has(job.id),
    )
}
