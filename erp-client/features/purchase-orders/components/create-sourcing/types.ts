import type { PurchaseOrderCreateFormApi } from "../../lib/purchase-order-create-form-types"
import type { SourcingSalesOrder } from "../../lib/purchase-order-create-model"

/** 表格与卡片共用的供给分配编辑上下文和行操作。 */
export type SourcingEditorProps = {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
    onAddSplit: (salesOrderLineId: string) => void
    onRemoveSplit: (rowKey: string) => void
}

/** 列表批量操作选择独立于表单中的「本次分配」。 */
export type SourcingBatchSelectionProps = {
    selectedProductIds: ReadonlySet<string>
    onToggleProducts: (ids: string[], selected: boolean) => void
}
