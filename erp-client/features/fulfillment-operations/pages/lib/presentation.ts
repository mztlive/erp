/**
 * 处理面的纯展示推导：责任状态、只读提示、来源返回地址。
 * 全部为无副作用函数，便于单独验证。
 */

import { displayText } from "@/features/fulfillment-operations/lib/readable-label"
import type { FulfillmentOperation } from "@/features/fulfillment-operations/types"

export type ResponsibilityStatus =
    | "blocked"
    | "assigned_to_me"
    | "assigned_to_other"

export function responsibilityStatus(
    operation: FulfillmentOperation | undefined,
    canExecute: boolean,
): ResponsibilityStatus {
    return operation?.gate.state === "BLOCKED"
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
        : operation?.gate.state === "BLOCKED"
          ? "业务条件未满足"
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
    const warehouse = displayText(operation.source.warehouseLabel)
    const fields: SourceContextField[] = []
    const salesOrderNo = displayText(operation.source.salesOrderNo)
    if (salesOrderNo) {
        fields.push({
            label: "销售单",
            value: salesOrderNo,
            href: salesOrderHref,
        })
    }
    const purchaseNo = displayText(operation.source.purchaseNo)
    if (purchaseNo) fields.push({ label: "采购单", value: purchaseNo })
    const customer = displayText(operation.source.customerLabel)
    if (customer) fields.push({ label: "客户", value: customer })
    const supplier = displayText(operation.source.supplierLabel)
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

export function sourceReturnHref(
    returnTo: string | undefined,
    fromWorkspace: string | undefined,
    operation: FulfillmentOperation | undefined,
): string | undefined {
    return (
        returnTo ??
        (fromWorkspace === "W05" && operation
            ? `/sales/orders/${operation.source.salesOrderId}`
            : fromWorkspace === "W08" && operation?.source.purchaseOrderId
              ? `/procurement/orders`
              : fromWorkspace === "W10"
                ? `/inventory`
                : undefined)
    )
}
