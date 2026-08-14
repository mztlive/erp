/**
 * W09 客户端投影的纯工具：时间换算、API 错误判定、单据 → 工作单的公共组装。
 * 无 React、无 HTTP 调用，只做纯转换，供 api/ 映射与队列投影复用。
 */

import type { ApiError } from "@/lib/api/errors"
import type {
    FulfillmentOperation,
    FulfillmentOperationType,
    FulfillmentSourceLine,
} from "@/features/fulfillment-operations/types"

export function secsToIso(secs: number | null | undefined): string {
    if (secs == null || secs === 0) return ""
    return new Date(secs * 1000).toISOString()
}

export function isApiError(error: unknown): error is ApiError {
    return (
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        "message" in error
    )
}

export function nowIso(): string {
    return new Date().toISOString()
}

export function dueLabelFromIso(iso: string): {
    dueLabel: string
    overdue: boolean
} {
    if (!iso) return { dueLabel: "—", overdue: false }
    const day = iso.slice(0, 10)
    const today = new Date().toISOString().slice(0, 10)
    if (day < today) return { dueLabel: "已超期", overdue: true }
    if (day === today) return { dueLabel: "今日到期", overdue: false }
    return { dueLabel: day, overdue: false }
}

export function emptySourceLine(
    partial: Partial<FulfillmentSourceLine> & {
        lineId: string
        salesOrderLineId: string
    },
): FulfillmentSourceLine {
    return {
        lineId: partial.lineId,
        salesOrderLineId: partial.salesOrderLineId,
        purchaseRevisionLineId: partial.purchaseRevisionLineId,
        itemName: partial.itemName ?? "",
        skuCode: partial.skuCode ?? "",
        unitCode: partial.unitCode ?? "",
        orderedQuantity: partial.orderedQuantity ?? "",
        remainingQuantity:
            partial.remainingQuantity ?? partial.orderedQuantity ?? "",
        stockReservationId: partial.stockReservationId,
        reservedQuantity: partial.reservedQuantity,
        availableOnHand: partial.availableOnHand,
        purchaseLineSalesAllocationId: partial.purchaseLineSalesAllocationId,
    }
}

export function baseOperation(
    partial: Omit<
        FulfillmentOperation,
        | "gate"
        | "actionBlockers"
        | "impact"
        | "summary"
        | "statusTone"
        | "statusLabel"
        | "priority"
        | "dueAt"
        | "dueLabel"
        | "overdue"
        | "responsibleLabel"
        | "sourceVersion"
        | "editVersion"
        | "source"
        | "lines"
        | "draft"
    > &
        Partial<FulfillmentOperation> & {
            operationType: FulfillmentOperationType
            operationId: string
        },
): FulfillmentOperation {
    const dueAt = partial.dueAt ?? nowIso()
    const { dueLabel, overdue } = dueLabelFromIso(dueAt)
    return {
        operationId: partial.operationId,
        operationType: partial.operationType,
        priority: partial.priority ?? 20,
        dueAt,
        dueLabel: partial.dueLabel ?? dueLabel,
        overdue: partial.overdue ?? overdue,
        statusLabel: partial.statusLabel ?? "待处理",
        statusTone: partial.statusTone ?? "info",
        responsibleLabel: partial.responsibleLabel ?? "",
        sourceVersion: partial.sourceVersion ?? "1",
        editVersion: partial.editVersion ?? 1,
        source: partial.source ?? {
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
        },
        gate: partial.gate ?? {
            state: "NOT_APPLICABLE",
            message: "",
        },
        lines: partial.lines ?? [],
        draft: partial.draft!,
        summary: partial.summary ?? "",
        impact: partial.impact ?? "",
        actionBlockers: partial.actionBlockers ?? [],
    }
}
