import type {
    InventoryAvailability,
    InventoryQuery,
    InventoryView,
} from "@/features/inventory/types"

export interface BuildListQueryInput {
    view: InventoryView
    qParam: string
    warehouseId: string | undefined
    skuId: string | undefined
    salesOrderLineId: string | undefined
    availability: InventoryAvailability
    movementType: string[]
    occurredFrom: string | undefined
    occurredTo: string | undefined
    cursorParam: string | undefined
    pageSize: number
    sortValue: string
    balanceIdParam: string | undefined
    adjustmentIdParam: string | undefined
}

export function buildListQuery(input: BuildListQueryInput): InventoryQuery {
    return {
        view: input.view,
        q: input.qParam || undefined,
        warehouseId: input.warehouseId,
        skuId: input.skuId,
        salesOrderLineId: input.salesOrderLineId,
        availability: input.availability,
        movementType: input.movementType,
        occurredFrom: input.occurredFrom,
        occurredTo: input.occurredTo,
        cursor: input.cursorParam,
        pageSize: input.pageSize,
        sort: input.sortValue.split(",").filter(Boolean),
        balanceId: input.balanceIdParam,
        adjustmentId: input.adjustmentIdParam,
    }
}
