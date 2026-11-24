import type { PurchaseOrderCreateFormApi } from "../../lib/purchase-order-create-form-types"
import type { SourcingSalesOrder } from "../../lib/purchase-order-create-model"

/** 表格与弹窗共用的供给分配编辑上下文和行操作。 */
export type SourcingEditorProps = {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
    onAddSplit: (salesOrderLineId: string) => void
    /** 按采购员在比价面板中选定的具体履约方案新增拆分来源，返回新增 rowKey。 */
    onAddSplitWithOption?: (
        salesOrderLineId: string,
        basisId: string,
    ) => string | undefined
    onRemoveSplit: (rowKey: string) => void
    /** 一键把剩余未覆盖需求或指定地址拆给新来源，返回新增 rowKey。 */
    onAddResidualSplit?: (
        salesOrderLineId: string,
        options?: { moveAddressKeys?: readonly string[] },
    ) => string | undefined
}

/**
 * 表格临时批量操作选区，与“是否参与本次提交”严格解耦：
 * 勾选仅决定批量换源/设仓/改交期/切参与状态的作用范围，不修改提交范围。
 */
export type SourcingBatchSelectionProps = {
    checkedProductIds: ReadonlySet<string>
    onToggleChecked: (ids: readonly string[], checked: boolean) => void
    onClearChecked: () => void
}

/** 商品级本次提交范围；所有拆分来源一起暂停，并可恢复原选择。 */
export type SourcingParticipationProps = {
    onSetParticipation: (ids: readonly string[], included: boolean) => void
}
