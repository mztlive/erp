import { apiGet } from "@/lib/api"

/** 一个可分配的收件地址。 */
export type PurchaseDeliveryAddressOption = {
    recipient: string
    phone: string
    address: string
    /** 扣除其他采购后本采购最多可分配数量。 */
    availableQuantity: string
}

/** 一个销售稳定行的交付安排范围。 */
export type PurchaseDeliveryLineOption = {
    salesOrderLineId: string
    lineNo: number
    itemName: string
    /** 销售当前履约期限，预计交付日不得晚于此日。 */
    salesDueDate: string
    /** 须按收件地址分配数量。 */
    requiresAddresses: boolean
    addresses: PurchaseDeliveryAddressOption[]
    /** 采购改单可填数量范围；采购尚未生效时缺省。 */
    quantityBound?: PurchaseQuantityBound
}

/** 采购改单一行的可填数量范围。 */
export type PurchaseQuantityBound = {
    /** 采购当前生效数量。 */
    baseQuantity: string
    /** 销售当前需要数量。 */
    salesQuantity: string
    /** 其他采购与库存已覆盖数量。 */
    otherCoveredQuantity: string
    /** 本采购已入库、发货或交付数量。 */
    fulfilledQuantity: string
    minQuantity: string
    maxQuantity: string
    /** 销售已调减或移除，本行必须调减。 */
    quantityAffected: boolean
}

type BackendPurchaseDeliveryOptions = {
    lines: Array<{
        sales_order_line_id: string
        line_no: number
        item_name: string
        sales_due_date: string
        requires_addresses: boolean
        addresses: Array<{
            recipient: string
            phone: string
            address: string
            available_quantity: string
        }>
        quantity_bound?: {
            base_quantity: string
            sales_quantity: string
            other_covered_quantity: string
            fulfilled_quantity: string
            min_quantity: string
            max_quantity: string
            quantity_affected: boolean
        } | null
    }>
}

/** 按销售当前版本读取采购各行的预计交付日上限与收件地址剩余量。 */
export async function fetchPurchaseDeliveryOptions(
    purchaseOrderId: string,
): Promise<PurchaseDeliveryLineOption[]> {
    const data = await apiGet<BackendPurchaseDeliveryOptions>(
        `/admin/purchase-orders/${encodeURIComponent(purchaseOrderId)}/delivery-options`,
    )
    return data.lines.map((line) => ({
        salesOrderLineId: line.sales_order_line_id,
        lineNo: line.line_no,
        itemName: line.item_name,
        salesDueDate: line.sales_due_date,
        requiresAddresses: line.requires_addresses,
        addresses: line.addresses.map((row) => ({
            recipient: row.recipient,
            phone: row.phone,
            address: row.address,
            availableQuantity: row.available_quantity,
        })),
        quantityBound: line.quantity_bound
            ? {
                  baseQuantity: line.quantity_bound.base_quantity,
                  salesQuantity: line.quantity_bound.sales_quantity,
                  otherCoveredQuantity:
                      line.quantity_bound.other_covered_quantity,
                  fulfilledQuantity: line.quantity_bound.fulfilled_quantity,
                  minQuantity: line.quantity_bound.min_quantity,
                  maxQuantity: line.quantity_bound.max_quantity,
                  quantityAffected: line.quantity_bound.quantity_affected,
              }
            : undefined,
    }))
}
