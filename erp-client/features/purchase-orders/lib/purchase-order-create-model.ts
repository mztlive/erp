/** 采购创建模型的公开入口；纯模型按工作区、推荐、预览和数量职责组织。 */
export type {
    PurchaseOrderPreview,
    PurchaseOrderPreviewLine,
    SourcingLineInput,
    SourcingOrderSummary,
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
    StockAllocationPreviewLine,
} from "./sourcing/types"
export {
    buildSourcingWorkspace,
    sourcingFormLinesReady,
    summarizeSourcingOrder,
} from "./sourcing/workspace"
export {
    assignBestSourcingOptions,
    buildDefaultSourcingLines,
    pickBestSourcingOption,
} from "./sourcing/recommendation"
export {
    commonSourcingOptionsForSelected,
    findSourcingOption,
} from "./sourcing/options"
export {
    buildPurchaseOrderPreviews,
    buildStockAllocationPreviews,
    previewLineAmounts,
    sumPreviewTotals,
} from "./sourcing/preview"
export { sourcingQuantityError } from "./sourcing/quantity"
