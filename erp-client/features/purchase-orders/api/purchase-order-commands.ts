import { apiPost } from "@/lib/api"
import { isApiError } from "@/lib/api/errors"
import { getErrorMessage } from "@/lib/api/errors"
import type { FormalActionResponse } from "@/features/purchase-orders/types"
import type {
    CreatedStockReservation,
    CreatePurchaseOrdersFromSourcingInput,
    CreatedPurchaseOrderDraft,
    PurchaseChangeOrderSummary,
    SavePurchaseOrderDraftInput,
    SubmitPurchaseOrderInput,
    VoidPurchaseOrderInput,
} from "@/features/purchase-orders/types"
import { formalActionFailure } from "./purchase-order-errors"
import type {
    BackendChangeStartResult,
    BackendCreateResult,
    BackendPurchaseChangeOrder,
    BackendSaveResult,
    BackendSourcingCreateResult,
    BackendSubmitResult,
    BackendVoidResult,
} from "./purchase-order-wire-types"
import { mapPurchaseChangeOrder } from "./purchase-order-mapping"
import type { PurchaseChangeDraftLine } from "./purchase-change-draft"

/** 草稿行补丁；预计交付日与收件分配只在调整时携带。 */
function toLinePatch(line: SavePurchaseOrderDraftInput["lines"][number]) {
    return {
        line_id: line.lineId,
        line_type: line.lineType,
        quantity: line.quantity,
        unit_cost_gross: line.unitCostGross,
        input_tax_rate: line.inputTaxRate,
        ...(line.expectedDeliveryDate
            ? { expected_delivery_date: line.expectedDeliveryDate }
            : {}),
        ...(line.deliveryAllocations
            ? { delivery_allocations: line.deliveryAllocations }
            : {}),
    }
}

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
                line_patches: input.lines.map(toLinePatch),
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

/**
 * 撤回尚未最终通过的采购单审批（`POST .../cancel-approval`）。
 * 服务端按单据乐观锁与运行中实例撤回；前端不依赖详情里的 instance 投影。
 */
export async function cancelPurchaseOrderApproval(input: {
    purchaseOrderId: string
    expectedLockVersion: number
    reason: string
    idempotencyKey: string
}): Promise<void> {
    await apiPost(
        `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/cancel-approval`,
        {
            expected_lock_version: input.expectedLockVersion,
            reason: input.reason.trim(),
            idempotency_key: input.idempotencyKey,
        },
    )
}

/**
 * 登记维持原采购（`POST .../source-update/maintain`）。
 * 销售变更影响本采购但无法调整时由采购负责人登记原因；服务端按记录版本防并发。
 */
export async function maintainPurchaseSource(input: {
    purchaseOrderId: string
    expectedVersion: number
    reason: string
}): Promise<void> {
    await apiPost(
        `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/source-update/maintain`,
        {
            expected_version: input.expectedVersion,
            reason: input.reason.trim(),
        },
    )
}

/** 标记采购单某商品的运费另计由供应商免收。 */
export async function waivePurchaseFreight(input: {
    purchaseOrderId: string
    salesOrderLineId: string
    reason: string
}): Promise<void> {
    await apiPost(
        `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/freight-waivers`,
        {
            sales_order_line_id: input.salesOrderLineId,
            reason: input.reason.trim(),
        },
    )
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
                line_patches: input.lines.map(toLinePatch),
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

/** 发起改单时的占位原因；弹窗中展示为空，要求重新填写。 */
export const PURCHASE_CHANGE_PLACEHOLDER_REASON = "采购变更"

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
                // 原因在改单弹窗填写，随提交更新；此处为占位。
                reason: PURCHASE_CHANGE_PLACEHOLDER_REASON,
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
export type SubmitPurchaseChangeInput = {
    purchaseChangeOrderId: string
    purchaseOrderId: string
    expectedLockVersion: number
    idempotencyKey: string
    paymentTermCode?: string
    /** 变更原因；缺省沿用原因。 */
    reason?: string
    lines?: PurchaseChangeDraftLine[]
}

export async function submitPurchaseChange(
    input: SubmitPurchaseChangeInput,
): Promise<FormalActionResponse<PurchaseChangeOrderSummary>> {
    try {
        const submitted = await apiPost<BackendPurchaseChangeOrder>(
            `/admin/purchase-change-orders/${encodeURIComponent(input.purchaseChangeOrderId)}/submit`,
            {
                expected_lock_version: input.expectedLockVersion,
                payment_term_code: input.paymentTermCode,
                reason: input.reason,
                // 已编辑的完整目标行原样提交；无编辑目标时沿用服务端基准。
                lines: input.lines ?? [],
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
 * @param input 来源销售单、任务、逐行履约依据与数量、幂等键。
 * @returns 正式命令结果；409 视为可采购数量冲突。
 */
export async function createPurchaseOrdersFromSourcing(
    input: CreatePurchaseOrdersFromSourcingInput,
): Promise<
    FormalActionResponse<{
        orders: CreatedPurchaseOrderDraft[]
        stockReservations: CreatedStockReservation[]
        workItemStatus: "OPEN" | "COMPLETED"
    }>
> {
    try {
        const data = await apiPost<BackendSourcingCreateResult>(
            "/admin/purchase-orders/from-sourcing",
            {
                work_item_id: input.workItemId,
                sales_order_id: input.salesOrderId,
                lines: input.lines.map((line) => ({
                    sales_order_line_id: line.salesOrderLineId,
                    basis_id: line.basisId,
                    source_type: line.sourceType,
                    target_warehouse_id: line.targetWarehouseId,
                    quantity: line.quantity,
                    expected_delivery_date: line.expectedDeliveryDate,
                    delivery_allocations: line.deliveryAllocations ?? [],
                })),
                idempotency_key: input.idempotencyKey,
            },
        )
        return {
            status: "succeeded",
            data: {
                orders: (data.orders ?? []).map(mapCreatedDraft),
                stockReservations: (data.stock_reservations ?? []).map(
                    (reservation) => ({
                        stockReservationId: reservation.stock_reservation_id,
                        salesOrderLineId: reservation.sales_order_line_id,
                        stockBalanceId: reservation.stock_balance_id,
                        warehouseId: reservation.warehouse_id,
                        quantity: reservation.quantity,
                    }),
                ),
                workItemStatus: data.work_item_status,
            },
            reference: data.reference,
        }
    } catch (error) {
        if (isApiError(error) && error.status === 409) {
            return {
                status: "failed",
                message: getErrorMessage(
                    error,
                    "可分配供给数量已更新，请刷新后重试",
                ),
                code: "CONFLICT",
            }
        }
        return formalActionFailure(error, input.idempotencyKey)
    }
}

/**
 * 作废采购改单草稿；审批中须先撤回。采购单保持当前生效版本，之后可重新发起改单。
 *
 * @param input 变更单与期望版本。
 */
export async function voidPurchaseChange(input: {
    purchaseChangeOrderId: string
    purchaseOrderId: string
    expectedLockVersion: number
}): Promise<PurchaseChangeOrderSummary> {
    const voided = await apiPost<BackendPurchaseChangeOrder>(
        `/admin/purchase-change-orders/${encodeURIComponent(input.purchaseChangeOrderId)}/void`,
        { expected_lock_version: input.expectedLockVersion },
    )
    return mapPurchaseChangeOrder(voided)
}
