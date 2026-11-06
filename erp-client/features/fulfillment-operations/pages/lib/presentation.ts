/**
 * 处理面的纯展示推导：责任状态、只读提示、来源返回地址。
 * 全部为无副作用函数，便于单独验证。
 */

import { displayText } from "@/features/fulfillment-operations/lib/readable-label"
import type {
    FulfillmentDraft,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"
import { displayName } from "@/lib/display-name"
import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"

/** 本次发货按单位分别汇总；不把件、箱等不同单位相加，也不把空物流计为已登记。 */
export function shipmentSummary(
    operation: FulfillmentOperation,
    draft: FulfillmentDraft,
): string | undefined {
    if (
        (draft.type !== "SUPPLIER_DIRECT" && draft.type !== "WAREHOUSE_SHIP") ||
        !draft.packing
    )
        return undefined
    try {
        const dispatch = draft.packing.dispatch.filter((row) =>
            row.items.some((item) => compareDecimal(item.quantity, "0", 6) > 0),
        )
        const items = dispatch
            .flatMap((row) => row.items)
            .filter((item) => compareDecimal(item.quantity, "0", 6) > 0)
        if (!items.length) return "尚未填写本次发货数量"
        const groups = new Map<string, string[]>()
        for (const item of items) {
            const unit =
                operation.lines.find(
                    (line) => line.lineId === item.delivery_line_id,
                )?.unitCode || ""
            groups.set(unit, [...(groups.get(unit) ?? []), item.quantity])
        }
        const quantity = groups.has("")
            ? `${new Set(items.map((item) => item.delivery_line_id)).size} 项商品`
            : [...groups]
                  .map(
                      ([unit, quantities]) =>
                          `${compactFixed(sumFixed(quantities, { maxScale: 6, outputScale: 6 }))} ${unit}`,
                  )
                  .join("、")
        return `本次发货 ${quantity} · ${dispatch.length} 个地址`
    } catch {
        return "本次发货数量待核对"
    }
}

export type ResponsibilityStatus =
    | "blocked"
    | "assigned_to_me"
    | "assigned_to_other"

export function responsibilityStatus(
    operation: FulfillmentOperation | undefined,
    canExecute: boolean,
): ResponsibilityStatus {
    return operation?.actionBlockers.some((item) => item.action === "POST")
        ? "blocked"
        : canExecute
          ? "assigned_to_me"
          : "assigned_to_other"
}

export function responsibilityStatusLabel(
    operation: FulfillmentOperation | undefined,
    canExecute: boolean,
): string {
    return !canExecute
        ? "只能查看"
        : operation?.actionBlockers.some((item) => item.action === "POST")
          ? "处理受阻"
          : "当前岗位可处理"
}

/** 只读角色看到的一句话：谁在处理、什么时候要交 */
export function readOnlyNote(
    operation: FulfillmentOperation | undefined,
): string {
    return operation
        ? `你只能查看。这条由 ${operation.responsibleLabel} 处理，${
              operation.overdue
                  ? `原定 ${operation.dueLabel}，已超期`
                  : `预计 ${operation.dueLabel} 前完成`
          }。`
        : "你只能查看这些单据的进度。"
}

export type SourceContextField = Readonly<{
    label: string
    value: string
    href?: string
}>

/**
 * 作业面来源摘要。空值和内部 id 不上屏，避免六宫格里一排破折号。
 * 待处理数量不在这里展示：明细表单里已经按行写了还剩多少。
 */
export function sourceContextFields(
    operation: FulfillmentOperation,
    salesOrderHref?: string,
): readonly SourceContextField[] {
    const warehouse = displayText(
        operation.source.warehouseLabel,
        operation.source.warehouseId,
    )
    const fields: SourceContextField[] = []
    const salesOrderNo = displayText(
        operation.source.salesOrderNo,
        operation.source.salesOrderId,
    )
    if (salesOrderNo) {
        fields.push({
            label: "销售单",
            value: salesOrderNo,
            href: salesOrderHref,
        })
    }
    const purchaseNo = displayText(
        operation.source.purchaseNo,
        operation.source.purchaseOrderId,
    )
    if (purchaseNo) fields.push({ label: "采购单", value: purchaseNo })
    const customer = displayName(operation.source.customerLabel)
    if (customer) fields.push({ label: "客户", value: customer })
    const supplier = displayName(operation.source.supplierLabel)
    if (supplier) fields.push({ label: "供应商", value: supplier })
    if (
        warehouse &&
        warehouse !== "不涉及仓库" &&
        operation.operationType !== "SUPPLIER_DIRECT" &&
        operation.operationType !== "SERVICE" &&
        operation.operationType !== "ELECTRONIC"
    ) {
        fields.push({ label: "仓库", value: warehouse })
    }
    return fields
}
