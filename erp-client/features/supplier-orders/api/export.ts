import { runDataTransfer, downloadTransfer } from "@/lib/data-transfer"
import type { ExportCommand, ExportJobResult } from "../types"
import { supplierOrderExportQuery } from "./list"
export async function createSupplierOrderExportJob(
    command: ExportCommand,
): Promise<ExportJobResult> {
    const file = await runDataTransfer(
        "supplier_orders",
        supplierOrderExportQuery(command.query),
        command.requestId,
    )
    downloadTransfer(file)
    return {
        jobId: file.jobId,
        requestId: command.requestId,
        rowCount:
            typeof file.metadata?.row_count === "number"
                ? file.metadata.row_count
                : command.rowCount,
        permissionVersion:
            typeof file.metadata?.scope_version === "string"
                ? file.metadata.scope_version
                : "",
        fieldSetId: command.fieldSetId,
        maskDisclaimer:
            "文件只包含获授权的列表字段，不含收件地址、电话及成本。",
        expiresAt:
            typeof file.metadata?.expires_at === "number"
                ? new Date(file.metadata.expires_at * 1000).toISOString()
                : "",
        downloadLabel: file.file_name,
        status: "succeeded",
    }
}
