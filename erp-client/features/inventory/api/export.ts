/**
 * W10 库存台账 · 台账导出 HTTP 入口。
 */

import { apiPost } from "@/lib/api"
import { secsToIso } from "@/features/inventory/api/display"
import type {
    BackendBackgroundJob,
    InventoryExportJob,
} from "@/features/inventory/api/dto"

export async function startInventoryExport(input: {
    total: number
    filterSummary: string
}): Promise<InventoryExportJob> {
    const now = Math.floor(Date.now() / 1000)
    const requestId = `inv-export-${now}-${Math.random().toString(36).slice(2, 8)}`
    const jobNo = `INV-EXP-${now}`
    const job = await apiPost<BackendBackgroundJob>("/admin/background-jobs", {
        job_no: jobNo,
        job_type: "export",
        domain_job_type: "INVENTORY_LEDGER_EXPORT",
        request_id: requestId,
        total_count: Math.max(1, input.total || 1),
        items: [
            {
                object_type: "INVENTORY_LEDGER",
                object_id: "filter",
                expected_hash: input.filterSummary.slice(0, 128),
            },
        ],
    })
    const statusMap: Record<string, InventoryExportJob["status"]> = {
        queued: "queued",
        pending: "queued",
        running: "running",
        succeeded: "succeeded",
        completed: "succeeded",
        failed: "failed",
        cancelled: "failed",
    }
    return {
        jobId: job.job_no || job.id,
        status: statusMap[job.status?.toLowerCase?.() ?? ""] ?? "queued",
        total: job.total_count ?? input.total,
        completed: job.processed_count ?? 0,
        filterSummary: input.filterSummary,
        createdAt: secsToIso(job.created_at) || new Date().toISOString(),
        downloadLabel: job.result_file_asset_id
            ? `库存台账导出-${job.job_no}`
            : undefined,
    }
}
