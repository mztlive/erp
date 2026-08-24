import { apiGet, apiPost } from "@/lib/api"
import type { FormalActionResponse } from "@/features/purchase-orders/types"
import type {
    CreatePurchaseOrderFromBasisInput,
    PurchaseChangeOrderSummary,
    ReviewPurchaseOrderInput,
    SavePurchaseOrderDraftInput,
    SubmitPurchaseOrderInput,
    VoidPurchaseOrderInput,
} from "@/features/purchase-orders/types"
import { formalActionFailure, isApiError } from "./purchase-order-errors"
import type {
    BackendCenter,
    BackendChangeStartResult,
    BackendCreateResult,
    BackendPurchaseChangeSubmitResult,
    BackendReviewResult,
    BackendSaveResult,
    BackendSubmitResult,
    BackendVoidResult,
} from "./purchase-order-wire-types"
import { fetchPurchaseChangeOrderDetail } from "./purchase-order-queries-api"

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
        // 合并当前中心行字段（后端整表替换；前端仅传补丁）
        const center = await apiGet<BackendCenter>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}`,
        )
        const patchById = new Map(input.lines.map((l) => [l.lineId, l]))

        const lines = (center.lines ?? []).map((line) => {
            const patch = patchById.get(line.line_id)
            const lineType =
                (patch?.lineType ?? line.line_type) === "LOGISTICS_FEE"
                    ? "LOGISTICS_FEE"
                    : "ITEM_SERVICE"
            return {
                line_type: lineType,
                procurement_confirmation_line_id:
                    line.procurement_confirmation_line_id ?? undefined,
                sku_id: line.sku_id ?? undefined,
                sku_revision_id: line.sku_revision_id ?? undefined,
                product_name: line.product_name ?? undefined,
                specification: line.specification ?? undefined,
                quantity: patch?.quantity ?? line.quantity ?? undefined,
                base_unit_code: line.base_unit_code ?? undefined,
                unit_cost_gross:
                    patch?.unitCostGross ?? line.unit_cost_gross ?? undefined,
                input_tax_rate:
                    patch?.inputTaxRate ?? line.input_tax_rate ?? "0",
                expected_delivery_date:
                    line.expected_delivery_date ?? undefined,
                sales_order_line_id: line.sales_order_line_id ?? undefined,
                sales_order_revision_line_id:
                    line.sales_order_revision_line_id ?? undefined,
                sales_order_submission_line_id:
                    line.sales_order_submission_line_id ?? undefined,
                allocated_quantity:
                    lineType === "ITEM_SERVICE"
                        ? (patch?.quantity ??
                          line.allocated_quantity ??
                          line.quantity ??
                          undefined)
                        : undefined,
                gross_amount:
                    lineType === "LOGISTICS_FEE"
                        ? (patch?.unitCostGross ?? line.gross_amount)
                        : undefined,
            }
        })

        const data = await apiPost<BackendSaveResult>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/draft`,
            {
                expected_lock_version: input.expectedLockVersion,
                payment_term_code: input.paymentTermCode,
                lines,
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
 * 把采购单中心当前行转成变更提交所需的完整目标行。
 *
 * 客户端不得选择定义或审批人，只冻结当前可见内容。
 *
 * @param center 原采购单对象中心 wire。
 */
const mapCenterLinesForChangeSubmit = (center: BackendCenter) =>
    (center.lines ?? []).map((line) => {
        const lineType =
            line.line_type === "LOGISTICS_FEE"
                ? "LOGISTICS_FEE"
                : "ITEM_SERVICE"
        return {
            line_type: lineType,
            procurement_confirmation_line_id:
                line.procurement_confirmation_line_id ?? undefined,
            sku_id: line.sku_id ?? undefined,
            sku_revision_id: line.sku_revision_id ?? undefined,
            product_name: line.product_name ?? undefined,
            specification: line.specification ?? undefined,
            quantity: line.quantity ?? undefined,
            base_unit_code: line.base_unit_code ?? undefined,
            unit_cost_gross: line.unit_cost_gross ?? undefined,
            input_tax_rate: line.input_tax_rate ?? "0",
            expected_delivery_date: line.expected_delivery_date ?? undefined,
            sales_order_submission_line_id:
                line.sales_order_submission_line_id ?? undefined,
            allocated_quantity: line.allocated_quantity ?? undefined,
            gross_amount:
                lineType === "LOGISTICS_FEE" ? line.gross_amount : undefined,
        }
    })

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
        const center = await apiGet<BackendCenter>(
            `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}`,
        )
        const submitted = await apiPost<BackendPurchaseChangeSubmitResult>(
            `/admin/purchase-change-orders/${encodeURIComponent(input.purchaseChangeOrderId)}/submit`,
            {
                expected_lock_version: input.expectedLockVersion,
                payment_term_code: center.payment_term_code,
                lines: mapCenterLinesForChangeSubmit(center),
                idempotency_key: input.idempotencyKey,
            },
        )
        const detail = await fetchPurchaseChangeOrderDetail(
            submitted.change_id || input.purchaseChangeOrderId,
        )
        return {
            status: "succeeded",
            data: detail,
            reference:
                submitted.reference || `CHANGE-SUB-${submitted.submission_no}`,
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
                draftLabel: data.purchase_no
                    ? `草稿 · ${data.purchase_no}`
                    : data.reference,
                lockVersion: data.lock_version,
            },
            reference: data.reference || data.purchase_no,
        }
    } catch (error) {
        if (isApiError(error) && error.status === 409) {
            return {
                status: "failed",
                message: "可采购数量已更新，请刷新后重试",
                code: "CONFLICT",
            }
        }
        return formalActionFailure(error, input.idempotencyKey)
    }
}
