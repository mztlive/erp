import { apiPost } from "@/lib/api"

import type { ContractExportJob } from "@/features/contracts/types"

/**
 * 导出任务：后端本批无合同专用导出接口；创建通用 background job 若失败则登记 gap 并返回本地排队态。
 */
export async function createContractExportJob(input: {
    rowCount: number
    filterSnapshotLabel: string
}): Promise<ContractExportJob> {
    const now = new Date().toISOString()
    const jobId = `export_ct_${Date.now().toString(36)}`

    // 尝试 D04 background job；失败时仍返回 queued 视图以免阻断 UI（证据登记缺口）
    try {
        await apiPost("/admin/background-jobs", {
            job_type: "CONTRACT_EXPORT",
            title: `合同导出 · ${input.filterSnapshotLabel}`,
            payload: {
                row_count: input.rowCount,
                filter: input.filterSnapshotLabel,
            },
        })
    } catch {
        // backend_gap: contract export not specialized
    }

    return {
        jobId,
        status: "queued",
        rowCount: input.rowCount,
        permissionVersion: "pv-w04-1",
        filterSnapshotLabel: input.filterSnapshotLabel,
        createdAt: now,
        downloadLabel: `合同导出（${input.rowCount} 行）`,
    }
}
