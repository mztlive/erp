import { runDataTransfer, downloadTransfer } from "@/lib/data-transfer"
import type { InventoryExportJob } from "./dto"
import { inventoryListRequestQuery } from "./list"
import { backendMovementTypeFilter } from "./display"
import {
    dateToUnixStart,
    dateToUnixEnd,
    sortTokenToBackend,
} from "./pagination"
import type { InventoryQuery } from "../types"
function exportQuery(query: InventoryQuery) {
    const sort = sortTokenToBackend(query.sort, query.view)
    const common = inventoryListRequestQuery(
        query,
        1,
        100,
        sort.sort_by,
        sort.sort_dir,
    )
    if (query.view === "balance")
        return {
            ...common,
            balance_id: query.balanceId,
            availability: query.availability,
        }
    if (query.view === "movement")
        return {
            ...common,
            movement_type: backendMovementTypeFilter(query.movementType),
            occurred_from: dateToUnixStart(query.occurredFrom),
            occurred_to: dateToUnixEnd(query.occurredTo),
        }
    if (query.view === "adjustment")
        return { ...common, adjustment_id: query.adjustmentId }
    return {
        q: query.q?.trim() || undefined,
        warehouse_id: query.warehouseId,
        sku_id: query.skuId,
        sales_order_line_id: query.salesOrderLineId,
        ...sort,
    }
}
export async function exportInventoryWithScope(query: InventoryQuery) {
    const result = await runDataTransfer(
        `inventory_${query.view}`,
        exportQuery(query),
    )
    downloadTransfer(result)
    return result
}
export async function startInventoryExport(input: {
    total: number
    filterSummary: string
    query?: InventoryQuery
}): Promise<InventoryExportJob> {
    if (!input.query) throw new Error("导出筛选缺失，请刷新后重试")
    const result = await exportInventoryWithScope(input.query)
    const total =
        typeof result.metadata?.row_count === "number"
            ? result.metadata.row_count
            : input.total
    return {
        jobId: result.jobId,
        status: "succeeded",
        total,
        completed: total,
        filterSummary: input.filterSummary,
        createdAt: new Date().toISOString(),
        downloadLabel: result.file_name,
    }
}
