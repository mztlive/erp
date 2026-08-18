/**
 * W10 库存台账 · 库存调整（草稿/提交/结果确认）HTTP 入口。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import type {
    AdjustmentDetailView,
    AdjustmentDraftView,
    AdjustmentReasonType,
    AdjustmentSubmitResponse,
} from "@/features/inventory/types"
import {
    adjustmentStatusMap,
    isApiError,
    isDraftAdjustmentStatus,
    reasonTypeBackend,
    secsToIso,
} from "@/features/inventory/api/display"
import {
    toAdjustmentDetailView,
    toDraftView,
} from "@/features/inventory/api/mappers"
import { fetchBalanceDetail } from "@/features/inventory/api/detail"
import type {
    BackendStockAdjustment,
    BackendStockAdjustmentDetail,
} from "@/features/inventory/api/dto"

/** 库存调整作为试点单据的合同 DocumentType。 */
export const STOCK_ADJUSTMENT_DOCUMENT_TYPE = "StockAdjustment" as const

/**
 * 构造库存调整提交请求。只允许单据版本与幂等键，不得带复核人或审批人。
 */
export const buildAdjustmentSubmitRequest = (input: {
    expectedVersion: number
    idempotencyKey: string
}): Readonly<{ expected_version: number; idempotency_key: string }> => ({
    expected_version: input.expectedVersion,
    idempotency_key: input.idempotencyKey,
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
}): Promise<AdjustmentDraftView> {
    const detail = await fetchBalanceDetail(input.balanceId)
    if (!detail) {
        const err: ApiError = {
            kind: "Http",
            message: "余额不存在",
            status: 404,
        }
        throw err
    }
    const b = detail.balance
    const adjustmentNo = `TZ${Date.now().toString(36).toUpperCase()}`
    // Create draft with a placeholder line (quantity 0 not allowed — use "1" placeholder until save)
    // Backend requires quantity > 0 and lines 1–100; draft created with default COUNT_LOSS/DAMAGE direction decrease qty "1"
    const created = await apiPost<BackendStockAdjustment>(
        "/admin/stock-adjustments",
        {
            adjustment_no: adjustmentNo,
            warehouse_id: b.warehouseId,
            reason_type: "STOCK_LOSS",
            lines: [
                {
                    sku_id: b.skuId,
                    quantity: "1",
                    direction: "DECREASE",
                },
            ],
        },
    )
    const full = await apiGet<BackendStockAdjustmentDetail>(
        `/admin/stock-adjustments/${encodeURIComponent(created.id)}`,
    )
    const draft = toDraftView(full, b.lockVersion)
    return {
        ...draft,
        balanceId: b.balanceId,
        warehouseName: b.warehouseName,
        skuCode: b.skuCode,
        skuName: b.skuName,
        baseUnit: b.baseUnit,
        quantity: "", // clear placeholder so user fills
    }
}

export async function submitAdjustment(input: {
    stockAdjustmentId: string
    expectedBalanceLockVersion: number
    seedBalanceLockVersion: number
    reasonType: AdjustmentReasonType
    reasonTypeLabel: string
    direction: "increase" | "decrease"
    quantity: string
    note: string
    occurredAt: string
    idempotencyKey: string
}): Promise<AdjustmentSubmitResponse> {
    try {
        // ensure reason is saved before submit
        const detail = await apiGet<BackendStockAdjustmentDetail>(
            `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}`,
        )
        if (isDraftAdjustmentStatus(detail.adjustment.status)) {
            await apiPut<BackendStockAdjustment>(
                `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}`,
                {
                    version: detail.adjustment.version,
                    reason_type: reasonTypeBackend(input.reasonType),
                },
            )
        }
        const latest = await apiGet<BackendStockAdjustmentDetail>(
            `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}`,
        )
        const submitted = await apiPost<BackendStockAdjustment>(
            `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}/submit`,
            buildAdjustmentSubmitRequest({
                expectedVersion: latest.adjustment.version,
                idempotencyKey: input.idempotencyKey,
            }),
        )
        const afterSubmit = await apiGet<BackendStockAdjustmentDetail>(
            `/admin/stock-adjustments/${encodeURIComponent(submitted.id)}`,
        )
        const approval = toAdjustmentDetailView(afterSubmit).approval
        const nextResponsible =
            approval.instance?.currentAssigneeName ??
            approval.instance?.currentAssignee ??
            approval.instance?.currentNodeName ??
            approval.definition?.nodes[0]?.assigneeName ??
            approval.definition?.nodes[0]?.name ??
            "当前审批人"
        return {
            status: "succeeded",
            outcome: {
                kind: "SUBMITTED_FOR_APPROVAL",
                stockAdjustmentId: submitted.id,
                adjustmentNo: submitted.adjustment_no,
                nextResponsible,
                currentNodeLabel:
                    approval.instance?.currentNodeName ??
                    approval.instance?.currentNode ??
                    approval.definition?.nodes[0]?.name,
                reference: submitted.adjustment_no,
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
                    message: "数据已变更，请刷新后重试",
                    latestLockVersion: input.expectedBalanceLockVersion,
                }
            }
            // OutcomeUnknown from backend maps to HTTP 500 with specific message
            if (
                error.status === 500 &&
                typeof error.message === "string" &&
                error.message.includes("暂无法确认")
            ) {
                return {
                    status: "unknown",
                    message: error.message,
                    idempotencyKey: input.idempotencyKey,
                }
            }
            return {
                status: "failed",
                code: String(error.status ?? "ERROR"),
                message: error.message,
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
                const approval = toAdjustmentDetailView(detail).approval
                const nextResponsible =
                    approval.instance?.currentAssigneeName ??
                    approval.instance?.currentAssignee ??
                    approval.instance?.currentNodeName ??
                    "当前审批人"
                return {
                    status: "succeeded",
                    outcome: {
                        kind: "SUBMITTED_FOR_APPROVAL",
                        stockAdjustmentId: detail.adjustment.id,
                        adjustmentNo: detail.adjustment.adjustment_no,
                        nextResponsible,
                        currentNodeLabel:
                            approval.instance?.currentNodeName ??
                            approval.instance?.currentNode,
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
