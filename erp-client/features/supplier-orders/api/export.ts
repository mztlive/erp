/**
 * W26 供应商订单 · 导出作业创建。
 */

import { apiPost } from "@/lib/api"
import type {
    ExportCommand,
    ExportJobResult,
} from "@/features/supplier-orders/types"
import { PERMISSION_VERSION, tsToIso } from "./mapping"
import type { BackendBackgroundJob } from "./wire-types"

export async function createSupplierOrderExportJob(
    command: ExportCommand,
): Promise<ExportJobResult> {
    const job = await apiPost<BackendBackgroundJob>("/admin/background-jobs", {
        job_no: `EXP-W26-${command.requestId.slice(-12)}`,
        job_type: "export",
        domain_job_type: "supplier_fulfillment_order_export",
        selection_snapshot_id: command.selectionSnapshotId || null,
        request_id: command.requestId,
        total_count: Math.max(1, command.rowCount || 1),
        items: [
            {
                object_type: "supplier_fulfillment_order",
                object_id: command.selectionSnapshotId || command.requestId,
            },
        ],
    })

    return {
        jobId: job.id,
        requestId: command.requestId,
        rowCount: command.rowCount,
        permissionVersion: PERMISSION_VERSION,
        fieldSetId: command.fieldSetId,
        maskDisclaimer:
            "导出使用系统筛选快照与字段权限打码：收货地址、手机号不会以明文写入文件。",
        expiresAt: job.result_expires_at
            ? tsToIso(job.result_expires_at)
            : new Date(Date.now() + 7 * 24 * 60 * 60 * 1000).toISOString(),
        downloadLabel: `供应商订单_${job.job_no ?? job.id}.csv`,
        status: job.status === "completed" ? "succeeded" : "queued",
    }
}
