/**
 * W10 库存台账 · 库存调整（草稿/提交/结果确认）HTTP 入口。
 */

import { apiGet, apiPost } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import type {
    AdjustmentDetailView,
    AdjustmentDraftView,
    AdjustmentReasonType,
    AdjustmentSubmitResponse,
} from "@/features/inventory/types"
import {
    adjustmentStatusMap,
    isApiError,
    reasonTypeBackend,
    secsToIso,
} from "@/features/inventory/api/display"
import {
    toAdjustmentDetailView,
    toDraftView,
} from "@/features/inventory/api/mappers"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type { BackendStockAdjustmentDetail } from "@/features/inventory/api/dto"

/** 库存调整作为试点单据的合同 DocumentType。 */
export const STOCK_ADJUSTMENT_DOCUMENT_TYPE = "StockAdjustment" as const

/**
 * 构造保存最终草稿并提交审批的原子命令，不得带复核人或审批人。
 */
export const buildAdjustmentSubmitRequest = (input: {
    expectedVersion: number
    balanceId: string
    expectedBalanceVersion: number
    lineId: string
    reasonType: AdjustmentReasonType
    direction: "increase" | "decrease"
    quantity: string
    note: string
    occurredAt: string
    idempotencyKey: string
}) => ({
    expected_version: input.expectedVersion,
    reason_type: reasonTypeBackend(input.reasonType),
    lines: [
        {
            line_id: input.lineId,
            quantity: input.quantity,
            direction: input.direction === "increase" ? "INCREASE" : "DECREASE",
        },
    ],
    balances: [
        {
            balance_id: input.balanceId,
            expected_version: input.expectedBalanceVersion,
        },
    ],
    note: input.note,
    occurred_at: Math.floor(new Date(input.occurredAt).getTime() / 1000),
    idempotency_key: input.idempotencyKey,
})

/**
 * 只读取实例投影上的当前节点与当前审批人。
 *
 * 缺失时省略，不得用定义首节点或默认称谓补位。
 */
export const readInstanceResponsibility = (
    approval?: DocumentApprovalView,
): {
    nextResponsible?: string
    currentNodeLabel?: string
} => ({
    nextResponsible:
        approval?.instance?.currentAssigneeName ??
        approval?.instance?.currentAssignee,
    currentNodeLabel:
        approval?.instance?.currentNodeName ?? approval?.instance?.currentNode,
})

/**
 * 查询库存调整单详情，含只读审批绑定。
 */
export async function fetchAdjustmentDetail(
    stockAdjustmentId: string,
): Promise<AdjustmentDetailView> {
    const detail = await apiGet<BackendStockAdjustmentDetail>(
        `/admin/stock-adjustments/${encodeURIComponent(stockAdjustmentId)}`,
    )
    return toAdjustmentDetailView(detail)
}

export async function createAdjustmentDraft(input: {
    balanceId: string
    balanceLockVersion: number
    warehouseId: string
    warehouseName: string
    skuId: string
    skuCode: string
    skuName: string
    baseUnit: string
}): Promise<AdjustmentDraftView> {
    const adjustmentNo = `TZ${Date.now().toString(36).toUpperCase()}`
    // Create draft with a placeholder line (quantity 0 not allowed — use "1" placeholder until save)
    // Backend requires quantity > 0 and lines 1–100; draft created with default COUNT_LOSS/DAMAGE direction decrease qty "1"
    const created = await apiPost<BackendStockAdjustmentDetail>(
        "/admin/stock-adjustments",
        {
            balance_id: input.balanceId,
            expected_balance_version: input.balanceLockVersion,
            adjustment_no: adjustmentNo,
            warehouse_id: input.warehouseId,
            reason_type: "STOCK_LOSS",
            lines: [
                {
                    sku_id: input.skuId,
                    quantity: "1",
                    direction: "DECREASE",
                },
            ],
        },
    )
    const draft = toDraftView(created, input.balanceLockVersion)
    return {
        ...draft,
        balanceId: input.balanceId,
        warehouseName: input.warehouseName,
        skuCode: input.skuCode,
        skuName: input.skuName,
        baseUnit: input.baseUnit,
        quantity: "", // clear placeholder so user fills
    }
}

export async function submitAdjustment(input: {
    stockAdjustmentId: string
    expectedDocumentVersion: number
    balanceId: string
    lineId: string
    expectedBalanceLockVersion: number
    reasonType: AdjustmentReasonType
    reasonTypeLabel: string
    direction: "increase" | "decrease"
    quantity: string
    note: string
    occurredAt: string
    idempotencyKey: string
}): Promise<AdjustmentSubmitResponse> {
    try {
        const submitted = await apiPost<BackendStockAdjustmentDetail>(
            `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}/submit`,
            buildAdjustmentSubmitRequest({
                expectedVersion: input.expectedDocumentVersion,
                balanceId: input.balanceId,
                expectedBalanceVersion: input.expectedBalanceLockVersion,
                lineId: input.lineId,
                reasonType: input.reasonType,
                direction: input.direction,
                quantity: input.quantity,
                note: input.note,
                occurredAt: input.occurredAt,
                idempotencyKey: input.idempotencyKey,
            }),
        )
        const approval = toAdjustmentDetailView(submitted).approval
        const responsibility = readInstanceResponsibility(approval)
        const adjustment = submitted.adjustment
        return {
            status: "succeeded",
            outcome: {
                kind: "SUBMITTED_FOR_APPROVAL",
                stockAdjustmentId: adjustment.id,
                adjustmentNo: adjustment.adjustment_no,
                nextResponsible: responsibility.nextResponsible,
                currentNodeLabel: responsibility.currentNodeLabel,
                reference: adjustment.adjustment_no,
                submittedAt: new Date().toISOString(),
                balanceLockVersion: input.expectedBalanceLockVersion,
            },
        }
    } catch (error) {
        if (isApiError(error)) {
            if (error.status === 409) {
                return {
                    status: "failed",
                    code: "BALANCE_LOCK_CONFLICT",
                    // 后端冲突码自带具体原因，前端透传不再改写
                    message: getErrorMessage(error, "数据已变更，请刷新后重试"),
                    latestLockVersion: input.expectedBalanceLockVersion,
                }
            }
            if (error.code === "OUTCOME_UNKNOWN") {
                return {
                    status: "unknown",
                    message: getErrorMessage(
                        error,
                        "操作结果暂无法确认，请先查询当前状态。",
                    ),
                    idempotencyKey: input.idempotencyKey,
                }
            }
            return {
                status: "failed",
                code: String(error.code ?? error.status ?? "ERROR"),
                message: getErrorMessage(error),
            }
        }
        throw error
    }
}

export async function resolveAdjustmentUnknown(input: {
    idempotencyKey: string
    stockAdjustmentId?: string
    expectedBalanceLockVersion?: number
}): Promise<AdjustmentSubmitResponse> {
    if (input.stockAdjustmentId) {
        try {
            const detail = await apiGet<BackendStockAdjustmentDetail>(
                `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}`,
            )
            const st = adjustmentStatusMap(detail.adjustment.status)
            if (st.status === "IN_APPROVAL" || st.status === "POSTED") {
                const responsibility = readInstanceResponsibility(
                    toAdjustmentDetailView(detail).approval,
                )
                return {
                    status: "succeeded",
                    outcome: {
                        kind: "SUBMITTED_FOR_APPROVAL",
                        stockAdjustmentId: detail.adjustment.id,
                        adjustmentNo: detail.adjustment.adjustment_no,
                        nextResponsible: responsibility.nextResponsible,
                        currentNodeLabel: responsibility.currentNodeLabel,
                        reference: detail.adjustment.adjustment_no,
                        submittedAt: secsToIso(detail.adjustment.created_at),
                        balanceLockVersion:
                            input.expectedBalanceLockVersion ??
                            detail.adjustment.version,
                    },
                }
            }
        } catch {
            // fall through
        }
    }
    return {
        status: "failed",
        code: "NO_PENDING",
        message: "未找到该任务号对应的处理中请求",
    }
}
