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
    CancelStockAdjustmentApprovalResult,
    StockAdjustmentCancelCommand,
    StockAdjustmentSubmitCommand,
} from "@/features/inventory/types"
import {
    isApiError,
    reasonTypeBackend,
    secsToIso,
} from "@/features/inventory/api/display"
import {
    toAdjustmentDetailView,
    toDraftView,
} from "@/features/inventory/api/mappers"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import type {
    BackendStockAdjustment,
    BackendStockAdjustmentDetail,
} from "@/features/inventory/api/dto"

/** 库存调整作为试点单据的合同 DocumentType。 */
export const STOCK_ADJUSTMENT_DOCUMENT_TYPE = "StockAdjustment" as const

export type CancelStockAdjustmentApprovalRequest = Readonly<{
    expected_version: string
    approval_process_instance_id: string
    expected_subject_version: string
    expected_instance_version: string
    expected_execution_version: string
    expected_task_version: string | null
    reason: string
    idempotency_key: string
}>

/**
 * 由详情令牌构造库存调整撤回请求。版本值原样透传，不允许从实例摘要补齐。
 */
export const buildCancelStockAdjustmentApprovalRequest = (input: {
    command: StockAdjustmentCancelCommand
    reason: string
    idempotencyKey: string
}): CancelStockAdjustmentApprovalRequest => ({
    expected_version: input.command.expectedVersion,
    approval_process_instance_id: input.command.approvalProcessInstanceId,
    expected_subject_version: input.command.expectedSubjectVersion,
    expected_instance_version: input.command.expectedInstanceVersion,
    expected_execution_version: input.command.expectedExecutionVersion,
    expected_task_version: input.command.expectedTaskVersion,
    reason: input.reason.trim(),
    idempotency_key: input.idempotencyKey,
})

/**
 * 构造保存最终草稿并提交审批的原子命令，不得带复核人或审批人。
 */
export const buildAdjustmentSubmitRequest = (input: {
    command: StockAdjustmentSubmitCommand
    balanceId: string
    expectedBalanceVersion: string
    lineId: string
    reasonType: AdjustmentReasonType
    direction: "increase" | "decrease"
    quantity: string
    note: string
    occurredAt: string
    idempotencyKey: string
}) => ({
    expected_version: input.command.expectedVersion,
    expected_subject_version: input.command.expectedSubjectVersion,
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

/**
 * 撤回库存调整审批。该端口返回库存调整单视图，不按审批命令响应解析。
 */
export async function cancelStockAdjustmentApproval(input: {
    stockAdjustmentId: string
    command: StockAdjustmentCancelCommand
    reason: string
    idempotencyKey: string
}): Promise<CancelStockAdjustmentApprovalResult> {
    const adjustment = await apiPost<BackendStockAdjustment>(
        `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}/cancel-approval`,
        buildCancelStockAdjustmentApprovalRequest(input),
    )
    return {
        stockAdjustmentId: adjustment.id,
        status: adjustment.status,
    }
}

export async function createAdjustmentDraft(input: {
    balanceId: string
    balanceLockVersion: string
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
    submitCommand: StockAdjustmentSubmitCommand
    balanceId: string
    lineId: string
    expectedBalanceLockVersion: string
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
                command: input.submitCommand,
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
    stockAdjustmentId: string
    expectedSubjectVersion: string
    expectedBalanceLockVersion?: string
}): Promise<AdjustmentSubmitResponse> {
    const query = new URLSearchParams({
        expected_subject_version: input.expectedSubjectVersion,
        idempotency_key: input.idempotencyKey,
    })
    try {
        // 只有服务端按精确 scope/key 找到并重验的收据才能确认原命令成功。
        // 禁止根据单据当前状态推测未知命令的结果。
        const detail = await apiGet<BackendStockAdjustmentDetail>(
            `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}/submit-result?${query.toString()}`,
        )
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
                ...(input.expectedBalanceLockVersion
                    ? {
                          balanceLockVersion: input.expectedBalanceLockVersion,
                      }
                    : {}),
            },
        }
    } catch (error) {
        if (isApiError(error) && error.status !== 404) {
            return {
                status: "unknown",
                message: getErrorMessage(
                    error,
                    "操作结果仍无法确认，请稍后使用原任务号再查询。",
                ),
                idempotencyKey: input.idempotencyKey,
            }
        }
    }
    return {
        status: "failed",
        code: "NO_PENDING",
        message: "未找到该任务号对应的处理中请求",
    }
}
