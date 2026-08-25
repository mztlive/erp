import { apiPost } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import type { FormalActionResponse } from "@/features/purchase-orders/types"
import type {
    CreatePurchaseOrderFromBasisInput,
    CreatePurchaseOrdersFromSourcingInput,
    CreatedPurchaseOrderDraft,
    PurchaseChangeOrderSummary,
    ReviewPurchaseOrderInput,
    SavePurchaseOrderDraftInput,
    SubmitPurchaseOrderInput,
    VoidPurchaseOrderInput,
} from "@/features/purchase-orders/types"
import { formalActionFailure, isApiError } from "./purchase-order-errors"
import type {
    BackendChangeStartResult,
    BackendCreateResult,
    BackendPurchaseChangeOrder,
    BackendReviewResult,
    BackendSaveResult,
    BackendSourcingCreateResult,
    BackendSubmitResult,
    BackendVoidResult,
} from "./purchase-order-wire-types"
import { mapPurchaseChangeOrder } from "./purchase-order-mapping"

export async function savePurchaseOrderDraft(
    input: SavePurchaseOrderDraftInput & { paymentTermLabel: string },
): Promise<
    FormalActionResponse<{
        lockVersion: number
        draftContentHash: string
        totals: { gross: string; net: string; tax: string }
    }>
> {
    try {
        const data = await apiPost<BackendSaveResult>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/draft`,
            {
                expected_lock_version: input.expectedLockVersion,
                payment_term_code: input.paymentTermCode,
                line_patches: input.lines.map((line) => ({
                    line_id: line.lineId,
                    line_type: line.lineType,
                    quantity: line.quantity,
                    unit_cost_gross: line.unitCostGross,
                    input_tax_rate: line.inputTaxRate,
                })),
                idempotency_key: input.idempotencyKey,
            },
        )

        return {
            status: "succeeded",
            data: {
                lockVersion: data.lock_version,
                // 后端无 draft_content_hash：用 reference 占位供前端提交前透传
                draftContentHash: data.reference || `v${data.lock_version}`,
                totals: data.totals,
            },
            reference: data.reference || `SAVED-V${data.lock_version}`,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}

export async function voidPurchaseOrderDraft(
    input: VoidPurchaseOrderInput,
): Promise<
    FormalActionResponse<{
        purchaseOrderId: string
        status: "VOIDED"
        lockVersion: number
    }>
> {
    try {
        const data = await apiPost<BackendVoidResult>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/void`,
            {
                expected_lock_version: input.expectedLockVersion,
                reason: input.reason,
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            data: {
                purchaseOrderId: data.purchase_order_id,
                status: data.status,
                lockVersion: data.lock_version,
            },
            reference: data.reference,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}

export async function submitPurchaseOrderForReview(
    input: SubmitPurchaseOrderInput,
): Promise<
    FormalActionResponse<{
        submissionId: string
        submissionNo: string
        subjectHash: string
        workItemId: string
        taskVersion: string
        subjectVersion: string
        purchaseNo: string
        lockVersion: number
    }>
> {
    try {
        const data = await apiPost<BackendSubmitResult>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/submit`,
            {
                expected_lock_version: input.expectedLockVersion,
                payment_term_code: input.paymentTermCode,
                line_patches: input.lines.map((line) => ({
                    line_id: line.lineId,
                    line_type: line.lineType,
                    quantity: line.quantity,
                    unit_cost_gross: line.unitCostGross,
                    input_tax_rate: line.inputTaxRate,
                })),
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            data: {
                submissionId: data.submission_id,
                submissionNo: data.submission_no,
                subjectHash: data.submission_id,
                workItemId: data.work_item_id,
                taskVersion: String(data.task_version),
                subjectVersion: data.subject_version,
                purchaseNo: data.purchase_no,
                lockVersion: data.lock_version,
            },
            reference: data.reference || `SUB-${data.submission_no}`,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}

export async function reviewPurchaseOrder(
    input: ReviewPurchaseOrderInput,
): Promise<
    FormalActionResponse<{
        reviewResult: "APPROVED" | "REJECTED"
        revisionId?: string
        revisionNo?: number
        payableOpenAmount?: string
        lockVersion: number
        reference: string
    }>
> {
    try {
        const decision = input.decision
        const data = await apiPost<BackendReviewResult>(
            `/admin/purchase-orders/${encodeURIComponent(decision.purchaseOrderId)}/review-decisions`,
            {
                work_item_id: input.workItemId,
                expected_task_version: input.expectedTaskVersion,
                expected_subject_version: input.expectedSubjectVersion,
                decision: {
                    purchase_order_id: decision.purchaseOrderId,
                    submission_id: decision.submissionId,
                    expected_purchase_order_lock_version:
                        decision.expectedPurchaseOrderLockVersion,
                    review_result: decision.reviewResult,
                    reason_code:
                        decision.reviewResult === "REJECTED"
                            ? decision.reasonCode
                            : undefined,
                    comment: decision.comment,
                },
                idempotency_key: input.idempotencyKey,
            },
        )
        if (
            data.work_item_id !== input.workItemId ||
            data.work_item_status !== "COMPLETED" ||
            data.subject_version !== input.expectedSubjectVersion ||
            data.review_result !== decision.reviewResult
        ) {
            return {
                status: "unknown",
                message:
                    "处理结果待确认。返回结果不完整，请使用本次操作重试或刷新确认。",
                idempotencyKey: input.idempotencyKey,
            }
        }
        return {
            status: "succeeded",
            data: {
                reviewResult:
                    data.review_result === "REJECTED" ? "REJECTED" : "APPROVED",
                revisionId: data.revision_id ?? undefined,
                revisionNo: data.revision_no ?? undefined,
                payableOpenAmount: undefined,
                lockVersion: data.lock_version,
                reference: data.reference,
            },
            reference: data.reference || `REVIEW-V${data.lock_version}`,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}

export async function startPurchaseChange(input: {
    purchaseOrderId: string
    expectedLockVersion: number
    idempotencyKey: string
}): Promise<
    FormalActionResponse<{ changeId: string; baseRevisionNo: number }>
> {
    try {
        const data = await apiPost<BackendChangeStartResult>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/changes`,
            {
                expected_lock_version: input.expectedLockVersion,
                // 前端契约未传 reason；后端必填
                reason: "采购变更",
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            data: {
                changeId: data.change_id,
                baseRevisionNo: data.base_revision_no,
            },
            reference: data.reference || `CHANGE-V${data.base_revision_no}`,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}

/**
 * 提交采购变更并启动统一审批。客户端不得选择定义或审批人。
 *
 * @param input 变更单版本、原采购单与幂等键。
 */
export async function submitPurchaseChange(input: {
    purchaseChangeOrderId: string
    purchaseOrderId: string
    expectedLockVersion: number
    idempotencyKey: string
}): Promise<FormalActionResponse<PurchaseChangeOrderSummary>> {
    try {
        const submitted = await apiPost<BackendPurchaseChangeOrder>(
            `/admin/purchase-change-orders/${encodeURIComponent(input.purchaseChangeOrderId)}/submit`,
            {
                expected_lock_version: input.expectedLockVersion,
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            data: mapPurchaseChangeOrder(submitted),
            reference: `CHANGE-${submitted.id}`,
        }
    } catch (error) {
        return formalActionFailure(error, input.idempotencyKey)
    }
}

export async function createPurchaseOrderFromBasis(
    input: CreatePurchaseOrderFromBasisInput,
): Promise<
    FormalActionResponse<{
        purchaseOrderId: string
        draftLabel: string
        lockVersion: number
    }>
> {
    try {
        const data = await apiPost<BackendCreateResult>(
            "/admin/purchase-orders",
            {
                basis_id: input.basisId,
                work_item_id: input.workItemId,
                purchase_type: input.purchaseType,
                payment_term_code: input.paymentTermCode,
                lines: input.lines.map((line) => ({
                    sales_order_line_id: line.salesOrderLineId,
                    quantity: line.quantity,
                })),
                idempotency_key: input.idempotencyKey,
            },
        )

        return {
            status: "succeeded",
            data: {
                purchaseOrderId: data.purchase_order_id,
                draftLabel: data.purchase_no || data.reference,
                lockVersion: data.lock_version,
            },
            reference: data.reference || data.purchase_no,
        }
    } catch (error) {
        if (isApiError(error) && error.status === 409) {
            return {
                status: "failed",
                // 后端冲突码自带具体原因，前端透传不再改写
                message: getErrorMessage(
                    error,
                    "可采购数量已更新，请刷新后重试",
                ),
                code: "CONFLICT",
            }
        }
        return formalActionFailure(error, input.idempotencyKey)
    }
}

/**
 * 把后端创建结果映射成页面可用的采购单摘要。
 *
 * @param data 单张已提交采购单创建结果。
 * @returns 采购单 ID、展示名和乐观锁版本。
 */
function mapCreatedDraft(data: BackendCreateResult): CreatedPurchaseOrderDraft {
    return {
        purchaseOrderId: data.purchase_order_id,
        draftLabel: data.purchase_no || data.reference,
        lockVersion: data.lock_version,
    }
}

/**
 * 按选源行一次创建多张采购单并提交审批。
 *
 * @param input 来源销售单、任务、逐行供应商与数量、幂等键。
 * @returns 正式命令结果；409 视为可采购数量冲突。
 */
export async function createPurchaseOrdersFromSourcing(
    input: CreatePurchaseOrdersFromSourcingInput,
): Promise<FormalActionResponse<{ orders: CreatedPurchaseOrderDraft[] }>> {
    try {
        const data = await apiPost<BackendSourcingCreateResult>(
            "/admin/purchase-orders/from-sourcing",
            {
                work_item_id: input.workItemId,
                sales_order_id: input.salesOrderId,
                lines: input.lines.map((line) => ({
                    sales_order_line_id: line.salesOrderLineId,
                    supplier_id: line.supplierId,
                    quantity: line.quantity,
                })),
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            data: {
                orders: (data.orders ?? []).map(mapCreatedDraft),
            },
            reference: data.reference,
        }
    } catch (error) {
        if (isApiError(error) && error.status === 409) {
            return {
                status: "failed",
                message: getErrorMessage(
                    error,
                    "可采购数量已更新，请刷新后重试",
                ),
                code: "CONFLICT",
            }
        }
        return formalActionFailure(error, input.idempotencyKey)
    }
}
