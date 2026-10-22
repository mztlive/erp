import type { PurchaseOrderCreateFormApi } from "../../lib/purchase-order-create-form-types"
import type { SourcingSalesOrder } from "../../lib/purchase-order-create-model"

/** 表格与卡片共用的供给分配编辑上下文和行操作。 */
export type SourcingEditorProps = {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
    onAddSplit: (salesOrderLineId: string) => void
    onRemoveSplit: (rowKey: string) => void
}

/** 商品勾选直接表示参与本次分配，批量操作复用同一范围。 */
export type SourcingBatchSelectionProps = {
    selectedProductIds: ReadonlySet<string>
    onToggleProducts: (ids: string[], selected: boolean) => void
}

/** 商品级本次处理范围；所有拆分来源一起暂停，并可恢复原选择。 */
export type SourcingParticipationProps = {
    onSetParticipation: (ids: readonly string[], included: boolean) => void
}
