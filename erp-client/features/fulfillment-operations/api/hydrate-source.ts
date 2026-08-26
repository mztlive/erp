/**
 * 履约作业面补全来源单号、往来方和品名。
 * 列表投影只有对象 id；这里按销售单 / 采购单 / 仓库再取一层可读字段。
 */

import { apiGet } from "@/lib/api"
import { displayText } from "@/features/fulfillment-operations/lib/readable-label"
import type {
    FulfillmentOperation,
    FulfillmentSourceLine,
} from "@/features/fulfillment-operations/types"

type SalesOrderLineSnapshot = Readonly<{
    id?: string
    sales_order_line_id?: string
    item_name_snapshot?: string
    unit_snapshot?: string | null
    base_unit_code?: string | null
}>

type SalesOrderDisplay = Readonly<{
    id: string
    order_no?: string
    working_copy?: {
        customer_name?: string
        lines?: SalesOrderLineSnapshot[]
    } | null
    submissions?: Array<{
        customer_name?: string
        lines?: SalesOrderLineSnapshot[]
    }>
    revisions?: Array<{
        customer_name?: string
        lines?: Array<{ item_name?: string; unit?: string | null }>
    }>
}>

type PurchaseOrderLineSnapshot = Readonly<{
    line_id?: string
    product_name?: string | null
    base_unit_code?: string | null
    sales_order_line_id?: string | null
    procurement_confirmation_line_id?: string | null
}>

type PurchaseOrderDisplay = Readonly<{
    id: string
    purchase_no?: string
    sales_order_id?: string
    sales_order_no?: string
    supplier_name?: string
    lines?: PurchaseOrderLineSnapshot[]
}>

type WarehouseDisplay = Readonly<{
    id: string
    warehouse_code?: string
}>

function firstText(
    ...values: Array<string | null | undefined>
): string | undefined {
    for (const value of values) {
        const shown = displayText(value)
        if (shown) return shown
    }
    return undefined
}

function salesLineName(
    detail: SalesOrderDisplay,
    salesOrderLineId: string,
): { itemName?: string; unitCode?: string } {
    const lines = [
        ...(detail.working_copy?.lines ?? []),
        ...(detail.submissions ?? []).flatMap(
            (submission) => submission.lines ?? [],
        ),
    ]
    const line = lines.find(
        (candidate) =>
            candidate.sales_order_line_id === salesOrderLineId ||
            candidate.id === salesOrderLineId,
    )
    return {
        itemName: firstText(line?.item_name_snapshot),
        unitCode: firstText(line?.unit_snapshot, line?.base_unit_code),
    }
}

function purchaseLineName(
    detail: PurchaseOrderDisplay,
    line: FulfillmentSourceLine,
): { itemName?: string; unitCode?: string } {
    const match = (detail.lines ?? []).find(
        (candidate) =>
            candidate.line_id === line.purchaseRevisionLineId ||
            candidate.sales_order_line_id === line.salesOrderLineId ||
            candidate.procurement_confirmation_line_id ===
                line.salesOrderLineId,
    )
    return {
        itemName: firstText(match?.product_name),
        unitCode: firstText(match?.base_unit_code),
    }
}

async function loadSalesOrder(
    salesOrderId: string,
): Promise<SalesOrderDisplay | null> {
    try {
        return await apiGet<SalesOrderDisplay>(
            `/admin/sales-orders/${encodeURIComponent(salesOrderId)}`,
        )
    } catch {
        return null
    }
}

async function loadPurchaseOrder(
    purchaseOrderId: string,
): Promise<PurchaseOrderDisplay | null> {
    try {
        return await apiGet<PurchaseOrderDisplay>(
            `/admin/purchase-orders/${encodeURIComponent(purchaseOrderId)}`,
        )
    } catch {
        return null
    }
}

async function loadWarehouse(warehouseId: string): Promise<string | undefined> {
    try {
        const warehouse = await apiGet<WarehouseDisplay>(
            `/admin/warehouses/${encodeURIComponent(warehouseId)}`,
        )
        return firstText(warehouse.warehouse_code)
    } catch {
        return undefined
    }
}

/**
 * 用关联单据补全作业面展示字段。补全失败时清空 id 占位，不把内部主键上屏。
 */
export async function enrichOperationDisplay(
    operation: FulfillmentOperation,
): Promise<FulfillmentOperation> {
    const salesOrderId = operation.source.salesOrderId.trim()
    const purchaseOrderId = operation.source.purchaseOrderId?.trim() ?? ""
    const warehouseId = operation.source.warehouseId?.trim() ?? ""

    const [salesOrder, purchaseOrder, warehouseLabel] = await Promise.all([
        salesOrderId ? loadSalesOrder(salesOrderId) : Promise.resolve(null),
        purchaseOrderId
            ? loadPurchaseOrder(purchaseOrderId)
            : Promise.resolve(null),
        warehouseId ? loadWarehouse(warehouseId) : Promise.resolve(undefined),
    ])

    const salesOrderNo = firstText(
        salesOrder?.order_no,
        purchaseOrder?.sales_order_no,
        operation.source.salesOrderNo,
    )
    const customerLabel = firstText(
        salesOrder?.working_copy?.customer_name,
        [...(salesOrder?.submissions ?? [])].at(-1)?.customer_name,
        [...(salesOrder?.revisions ?? [])].at(-1)?.customer_name,
        operation.source.customerLabel,
    )
    const purchaseNo = firstText(
        purchaseOrder?.purchase_no,
        operation.source.purchaseNo,
    )
    const supplierLabel = firstText(
        purchaseOrder?.supplier_name,
        operation.source.supplierLabel,
    )
    const linkedSalesOrderId =
        firstText(salesOrder?.id, purchaseOrder?.sales_order_id) ?? salesOrderId

    const lines = operation.lines.map((line) => {
        const fromPurchase = purchaseOrder
            ? purchaseLineName(purchaseOrder, line)
            : {}
        const fromSales = salesOrder
            ? salesLineName(salesOrder, line.salesOrderLineId)
            : {}
        return {
            ...line,
            itemName:
                firstText(
                    line.itemName,
                    fromPurchase.itemName,
                    fromSales.itemName,
                ) ?? "",
            unitCode:
                firstText(
                    line.unitCode,
                    fromPurchase.unitCode,
                    fromSales.unitCode,
                ) ?? "",
        }
    })

    const nextWarehouseLabel = firstText(
        warehouseLabel,
        operation.source.warehouseLabel,
    )

    return {
        ...operation,
        source: {
            ...operation.source,
            salesOrderId: linkedSalesOrderId || operation.source.salesOrderId,
            salesOrderNo: salesOrderNo ?? "",
            customerLabel: customerLabel ?? "",
            purchaseNo,
            supplierLabel,
            warehouseLabel: nextWarehouseLabel,
        },
        lines,
        draft:
            operation.draft.type === "RECEIPT" ||
            operation.draft.type === "WAREHOUSE_SHIP"
                ? {
                      ...operation.draft,
                      warehouseLabel:
                          nextWarehouseLabel ?? operation.draft.warehouseLabel,
                  }
                : operation.draft,
    }
}
