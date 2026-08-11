import { apiPost } from "@/lib/api"
import {
    PERMISSION_VERSION,
    type BackendBackgroundJob,
    type ExportJobResult,
} from "./contracts"
import { formatIsoNow } from "./mappers"

export async function createSalesOrderExportJob(input: {
    rowCount: number
}): Promise<ExportJobResult> {
    const jobNo = `EXP-SO-${Date.now().toString(36).toUpperCase()}`
    const requestId =
        typeof crypto !== "undefined" && "randomUUID" in crypto
            ? crypto.randomUUID()
            : `export-${Date.now()}`

    const job = await apiPost<BackendBackgroundJob>("/admin/background-jobs", {
        job_no: jobNo,
        job_type: "export",
        domain_job_type: "SALES_ORDER_EXPORT",
        domain_job_id: null,
        selection_snapshot_id: null,
        request_id: requestId,
        input_file_asset_id: null,
        total_count: Math.max(1, input.rowCount),
        items: [
            {
                object_type: "sales_order",
                object_id: null,
                expected_version: null,
                expected_hash: null,
                worksheet_name: null,
                source_row_no: null,
                source_column_name: null,
            },
        ],
    })

    return {
        jobId: job.id ?? jobNo,
        status: "queued",
        rowCount: input.rowCount,
        permissionVersion: PERMISSION_VERSION,
        createdAt: formatIsoNow(),
        downloadLabel: `销售单导出_${jobNo}`,
    }
}
