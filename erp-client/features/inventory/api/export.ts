/**
 * W10 库存台账 · 台账导出 HTTP 入口。
 */

import { apiPost } from "@/lib/api"
import { fetchCompleteList } from "@/lib/collect-pages"
import { buildListCsv, downloadListCsv } from "@/lib/list-export"
import { secsToIso } from "@/features/inventory/api/display"
import type {
    BackendBackgroundJob,
    BackendStockAdjustment,
    BackendStockBalance,
    BackendStockMovement,
    InventoryExportJob,
} from "@/features/inventory/api/dto"
import { inventoryListRequestQuery } from "@/features/inventory/api/list"
import type { InventoryQuery } from "@/features/inventory/types"

const EXPORT_PATH: Record<
    Exclude<InventoryQuery["view"], "reservation">,
    string
> = {
    balance: "/admin/stock-balances",
    movement: "/admin/stock-movements",
    adjustment: "/admin/stock-adjustments",
}

export async function exportInventoryWithScope(
    query: InventoryQuery,
): Promise<void> {
    if (query.view === "reservation") {
        throw new Error("销售预占导出仍使用原入口")
    }
    const path = EXPORT_PATH[query.view]
    const params = inventoryListRequestQuery(
        query,
        1,
        100,
        undefined,
        undefined,
    )
    delete params.page
    delete params.page_size
    if (query.view === "balance") {
        const result = await fetchCompleteList<BackendStockBalance>(path, {
            ...params,
            balance_id: query.balanceId,
            availability: query.availability,
        })
        downloadListCsv(
            buildListCsv([
                ["仓库", "SKU", "现存", "预占", "可用"],
                ...result.items.map((row) => [
                    row.warehouse_name,
                    row.sku_code,
                    row.on_hand_quantity,
                    row.reserved_quantity,
                    row.available_quantity,
                ]),
            ]),
            "库存余额.csv",
        )
        return
    }
    if (query.view === "movement") {
        const result = await fetchCompleteList<BackendStockMovement>(path, {
            ...params,
        })
        downloadListCsv(
            buildListCsv([
                ["仓库", "SKU", "类型", "数量", "经办人"],
                ...result.items.map((row) => [
                    row.warehouse_id,
                    row.sku_id,
                    row.movement_type,
                    row.quantity,
                    row.recorded_by ?? "",
                ]),
            ]),
            "库存流水.csv",
        )
        return
    }
    const result = await fetchCompleteList<BackendStockAdjustment>(path, {
        ...params,
        adjustment_id: query.adjustmentId,
    })
    downloadListCsv(
        buildListCsv([
            ["调整单", "仓库", "经办人", "申请人", "当前审批人", "状态"],
            ...result.items.map((row) => [
                row.adjustment_no,
                row.warehouse_id,
                row.prepared_by,
                row.submitted_by ?? "",
                row.current_assignee ?? "",
                row.status,
            ]),
        ]),
        "库存调整.csv",
    )
}

export async function startInventoryExport(input: {
    total: number
    filterSummary: string
    query?: InventoryQuery
}): Promise<InventoryExportJob> {
    if (input.query && input.query.view !== "reservation") {
        await exportInventoryWithScope(input.query)
        return {
            jobId: "inventory-export",
            status: "succeeded",
            total: input.total,
            completed: input.total,
            filterSummary: input.filterSummary,
            createdAt: new Date().toISOString(),
        }
    }
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
