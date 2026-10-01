import type {
    FulfillmentResponsibility,
    PurchaseType,
    SupplySourceType,
} from "@/features/purchase-orders/types"

/** 一条销售明细可选的合格履约方案。 */
export type SourcingSupplierOption = Readonly<{
    sourceType: SupplySourceType
    supplierId: string
    supplierName: string
    basisId: string
    workItemId: string
    purchaseType: PurchaseType
    fulfillmentResponsibility: FulfillmentResponsibility
    paymentTermCode: string
    paymentTermLabel: string
    businessCategory?: string
    stockBalanceId?: string
    warehouseId?: string
    warehouseName?: string
    sourceAvailableQuantity?: string
    unitCostGross: string
    inputTaxRate: string
    maxCreateQuantity: string
    expectedDeliveryDate: string
}>

/** 选源工作区中的一条销售明细。 */
export type SourcingProductLine = Readonly<{
    salesOrderLineId: string
    itemName: string
    itemSku?: string
    unit: string
    quantityScale?: number | null
    salesQuantity: string
    coveredQuantity: string
    remainingQuantity: string
    /** 销售承诺的最晚交付日，采购预计交付日不得晚于此日。 */
    deliveryDeadline: string
    salesAllocationLabel: string
    options: readonly SourcingSupplierOption[]
}>

/** 一张可分配供给的销售单及其剩余明细。 */
export type SourcingSalesOrder = Readonly<{
    salesOrderId: string
    salesOrderNo: string
    customerName: string
    contractNumber?: string
    salesOwnerName?: string
    workItemId: string
    lines: readonly SourcingProductLine[]
}>

/** 建单表单中一条可编辑选源行。 */
export type SourcingLineInput = {
    rowKey: string
    salesOrderLineId: string
    selected: boolean
    quantity: string
    basisId: string
    targetWarehouseId?: string
    targetWarehouseName?: string
    expectedDeliveryDate: string
}

/** 确认创建前按拆分维度预览的一张采购单。 */
export type PurchaseOrderPreview = Readonly<{
    key: string
    supplierId: string
    supplierName: string
    purchaseType: PurchaseType
    fulfillmentResponsibility: FulfillmentResponsibility
    paymentTermCode: string
    paymentTermLabel: string
    workItemId: string
    basisId: string
    targetWarehouseId?: string
    targetWarehouseName?: string
    lines: readonly PurchaseOrderPreviewLine[]
    totals: Readonly<{ gross: string; net: string; tax: string }>
}>

/** 预览采购单中的一行。 */
export type PurchaseOrderPreviewLine = Readonly<{
    salesOrderLineId: string
    itemName: string
    itemSku?: string
    unit: string
    quantity: string
    unitCostGross: string
    inputTaxRate: string
    expectedDeliveryDate: string
    grossAmount: string
    netAmount: string
    taxAmount: string
}>

/** 确认前展示的一条现有库存分配。 */
export type StockAllocationPreviewLine = Readonly<{
    salesOrderLineId: string
    itemName: string
    warehouseName: string
    quantity: string
    unit: string
}>

/** 选源销售单的汇总事实，供来源区密集展示。 */
export type SourcingOrderSummary = Readonly<{
    lineCount: number
    coveredLineCount: number
    uniqueSupplierCount: number
    purchaseTypes: readonly PurchaseType[]
    fulfillmentResponsibilities: readonly FulfillmentResponsibility[]
    paymentTermLabels: readonly string[]
    businessCategories: readonly string[]
    minEstimatedGross: string
}>
