import {
    deliveryRowsMatchQuantity,
    positiveDeliveryRows,
    sameDeliveryRows,
} from "@/features/purchase-orders/lib/delivery-adjustment"
import type {
    PurchaseDeliveryAllocation,
    PurchaseOrderCenterView,
    SavePurchaseOrderDraftInput,
} from "@/features/purchase-orders/types"
import { compareDecimal } from "@/lib/fixed-decimal"

/** 草稿行的本地编辑缓存。 */
export type DraftLineEdit = {
    quantity?: string
    unitCostGross?: string
    inputTaxRate: string
    /** 调整后的预计交付日。 */
    expectedDeliveryDate?: string
    /** 按销售当前收件地址调整后的分配；未调整时缺省。 */
    deliveryAllocations?: PurchaseDeliveryAllocation[]
}

export type DraftLineEdits = Record<string, DraftLineEdit>

type CenterLine = PurchaseOrderCenterView["currentContent"]["lines"][number]

function isZero(value: string | undefined) {
    if (!value) return false
    try {
        return compareDecimal(value.trim(), "0", 6) === 0
    } catch {
        return false
    }
}

/** 调整后的收件分配；与当前分配一致时视为未调整。 */
function adjustedRows(line: CenterLine, edit: DraftLineEdit | undefined) {
    if (!edit?.deliveryAllocations) return undefined
    const rows = positiveDeliveryRows(edit.deliveryAllocations)
    return sameDeliveryRows(rows, line.deliveryAllocations ?? [])
        ? undefined
        : rows
}

/** 调整后的预计交付日；与当前一致时视为未调整。 */
function adjustedDate(line: CenterLine, edit: DraftLineEdit | undefined) {
    const value = edit?.expectedDeliveryDate?.trim()
    return value && value !== line.expectedDeliveryDate ? value : undefined
}

/**
 * 校验收件分配与采购数量一致。
 *
 * @returns 第一处不一致的提示；全部一致时为 undefined。
 */
export function draftDeliveryProblem(
    order: PurchaseOrderCenterView,
    lineEdits: DraftLineEdits,
): string | undefined {
    for (const line of order.currentContent.lines) {
        if (line.lineType !== "ITEM_SERVICE") continue
        const edit = lineEdits[line.lineId]
        const quantity = edit?.quantity ?? line.quantity ?? ""
        if (isZero(quantity)) continue
        if (edit?.deliveryAllocations) {
            if (!deliveryRowsMatchQuantity(edit.deliveryAllocations, quantity))
                return `「${line.itemName}」各收件地址交付数量合计须等于采购数量。`
            continue
        }
        if (
            (line.deliveryAllocations?.length ?? 0) > 0 &&
            quantity !== line.quantity
        )
            return `「${line.itemName}」数量已调整，请同步调整各收件地址的交付数量。`
    }
    return undefined
}

/** 由草稿行与本地编辑生成保存或提交的行补丁。 */
export function draftLinePayload(
    order: PurchaseOrderCenterView,
    lineEdits: DraftLineEdits,
): SavePurchaseOrderDraftInput["lines"] {
    return order.currentContent.lines.map((line) => {
        const edit = lineEdits[line.lineId]
        const quantity = edit?.quantity ?? line.quantity
        return {
            lineId: line.lineId,
            lineType: line.lineType,
            quantity,
            unitCostGross: edit?.unitCostGross ?? line.unitCostGross,
            inputTaxRate: edit?.inputTaxRate ?? line.inputTaxRate,
            logisticsFeeReason: line.logisticsFeeReason,
            expectedDeliveryDate: adjustedDate(line, edit),
            deliveryAllocations: isZero(quantity)
                ? undefined
                : adjustedRows(line, edit),
        }
    })
}

/** 草稿行是否有未保存的编辑。 */
export function draftLinesDirty(
    order: PurchaseOrderCenterView,
    lineEdits: DraftLineEdits,
) {
    return order.currentContent.lines.some((line) => {
        const edit = lineEdits[line.lineId]
        if (!edit) return false
        return (
            (edit.quantity ?? line.quantity) !== line.quantity ||
            (edit.unitCostGross ?? line.unitCostGross) !== line.unitCostGross ||
            edit.inputTaxRate !== line.inputTaxRate ||
            adjustedDate(line, edit) !== undefined ||
            adjustedRows(line, edit) !== undefined
        )
    })
}
