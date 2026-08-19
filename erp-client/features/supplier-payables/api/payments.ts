/**
 * W12 供应商往来 · 付款相关请求（登记草稿、提交审批、冲正）。
 * 幂等结果缓存见 api/shared。过账只由最终通过动作内部消费。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { BackendSupplierPayment } from "@/features/supplier-payables/api/mappers"
import {
    errorMessage,
    sessions,
    submitIdempotency,
} from "@/features/supplier-payables/api/shared"
import { ensurePaymentReversalDraft } from "@/features/supplier-payables/api/reversals"
import {
    buildSupplierPaymentSubmitRequest,
    mapSupplierPaymentApproval,
} from "@/features/supplier-payables/lib/supplier-payment-approval"
import type {
    AllocationSessionView,
    FormalSubmitResult,
    PostPaymentInput,
    ReversePaymentInput,
} from "@/features/supplier-payables/types"

function isPostedPaymentStatus(status?: string): boolean {
    return status === "posted" || status === "POSTED" || status === "reversed"
}

function isInApprovalPaymentStatus(status?: string): boolean {
    return (
        status === "IN_APPROVAL" ||
        status === "in_approval" ||
        status === "pending_review" ||
        status === "PENDING_REVIEW"
    )
}

function failedPayment(
    code: string,
    message: string,
): Extract<FormalSubmitResult, { status: "failed" }> {
    return {
        status: "failed",
        title: "付款失败",
        description: message,
        errorCode: code,
    }
}

/**
 * 把已创建付款草稿写回会话，供绑定卡只读展示。
 *
 * @param session 当前核销会话。
 * @param payment 服务端付款视图。
 */
function rememberCreatedPayment(
    session: AllocationSessionView,
    payment: BackendSupplierPayment,
): AllocationSessionView {
    const next: AllocationSessionView = {
        ...session,
        existingPaymentId: payment.id,
        existingDocumentNo: payment.payment_no,
        existingAmount: payment.amount,
        existingUnallocated: payment.unallocated_amount,
        existingPaymentVersion: payment.version,
        approval: mapSupplierPaymentApproval(payment.approval),
    }
    sessions.set(session.draftSessionId, next)
    return next
}

/**
 * 创建供应商付款草稿并只读展示服务端绑定。提交仍走独立确认。
 *
 * @param input 创建草稿所需字段。
 */
async function createSupplierPaymentDraft(input: {
    supplierId: string
    paidAt: string
    amount: string
    bankReference: string
    idempotencyKey: string
}): Promise<BackendSupplierPayment> {
    const paidAtSecs = input.paidAt
        ? Math.floor(new Date(input.paidAt).getTime() / 1000)
        : Math.floor(Date.now() / 1000)
    return apiPost<BackendSupplierPayment>("/admin/supplier-payments", {
        payment_no: `FK-${input.idempotencyKey.slice(-8)}`,
        supplier_id: input.supplierId,
        paid_at: paidAtSecs,
        amount: input.amount,
        bank_reference: input.bankReference || undefined,
    })
}

/**
 * 读取或创建付款草稿，并在会话存在时写回只读审批绑定。
 *
 * 已存在草稿时只刷新版本与绑定，不得调用过账旁路。
 *
 * @param input 当前核销会话与付款事实。
 */
export async function ensureSupplierPaymentDraft(input: {
    draftSessionId: string
    supplierId: string
    paidAt: string
    amount: string
    bankReference: string
    existingPaymentId?: string
    idempotencyKey: string
}): Promise<
    | {
          status: "succeeded"
          payment: BackendSupplierPayment
          session?: AllocationSessionView
      }
    | Extract<FormalSubmitResult, { status: "failed" }>
> {
    const session = sessions.get(input.draftSessionId)
    if (session && session.track !== "payment") {
        return failedPayment("NOT_PAYMENT", "当前核销不是付款。")
    }

    const paymentId = input.existingPaymentId ?? session?.existingPaymentId
    if (paymentId) {
        const latest = await apiGet<BackendSupplierPayment>(
            `/admin/supplier-payments/${encodeURIComponent(paymentId)}`,
        )
        if (isPostedPaymentStatus(latest.status)) {
            return failedPayment(
                "PAYMENT_ALREADY_POSTED",
                "已过账付款不得再次过账。未分配余额请另登新付款。",
            )
        }
        if (isInApprovalPaymentStatus(latest.status)) {
            return failedPayment(
                "PAYMENT_IN_APPROVAL",
                "付款正在审批中，不能重复提交。",
            )
        }
        return {
            status: "succeeded",
            payment: latest,
            session: session
                ? rememberCreatedPayment(session, latest)
                : undefined,
        }
    }

    const created = await createSupplierPaymentDraft(input)
    return {
        status: "succeeded",
        payment: created,
        session: session ? rememberCreatedPayment(session, created) : undefined,
    }
}

/**
 * 提交供应商付款审批。只走 `/submit`，不得调用已关闭的过账旁路。
 *
 * @param input 提交所需字段。
 */
export async function submitPayment(
    input: PostPaymentInput,
): Promise<FormalSubmitResult> {
    const cached = submitIdempotency.get(input.idempotencyKey)
    if (cached) return cached

    const targets = input.targets.filter(
        (t) => t.amount && Number(t.amount) > 0,
    )
    if (targets.length === 0) {
        return failedPayment(
            "NEED_ALLOCATION",
            "提交审批至少需要一条核销分配。",
        )
    }

    try {
        const ensured = await ensureSupplierPaymentDraft({
            draftSessionId: input.draftSessionId,
            supplierId: input.supplierId,
            paidAt: input.paidAt,
            amount: input.amount,
            bankReference: input.bankReference,
            existingPaymentId: input.existingPaymentId,
            idempotencyKey: input.idempotencyKey,
        })
        if (ensured.status !== "succeeded") {
            return ensured
        }
        const paymentId = ensured.payment.id
        const expectedVersion =
            input.expectedVersion ?? ensured.payment.version
        if (!paymentId || !expectedVersion) {
            return failedPayment(
                "PAYMENT_DRAFT_MISSING",
                "付款草稿尚未创建，请刷新后重试。",
            )
        }

        const submitted = await apiPost<BackendSupplierPayment>(
            `/admin/supplier-payments/${encodeURIComponent(paymentId)}/submit`,
            buildSupplierPaymentSubmitRequest({
                expectedVersion,
                idempotencyKey: input.idempotencyKey,
                allocations: targets.map((t) => ({
                    payableEntryId: t.payableEntryId ?? t.payableAccountId,
                    allocatedAmount: t.amount,
                })),
            }),
        )
        const approval = mapSupplierPaymentApproval(submitted.approval)
        const draft = ensured.session
        if (draft) {
            sessions.set(input.draftSessionId, {
                ...draft,
                existingPaymentId: submitted.id,
                existingDocumentNo: submitted.payment_no,
                existingPaymentVersion: submitted.version,
                existingUnallocated: submitted.unallocated_amount,
                approval,
            })
        }
        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "付款已提交审批",
            description: "已进入审批。全部节点通过后过账并核销。",
            reference: submitted.payment_no,
            operationId: input.idempotencyKey,
            documentNo: submitted.payment_no,
            unallocatedAmount: submitted.unallocated_amount,
            allocatedTotal: submitted.allocated_total,
            returnTo: draft?.returnTo,
            approval,
            subjectStatus: submitted.status,
        }
        submitIdempotency.set(input.idempotencyKey, result)
        return result
    } catch (err) {
        return {
            status: "failed",
            title: "付款失败",
            description: errorMessage(err, "付款提交失败"),
            errorCode: "HTTP_ERROR",
        }
    }
}

/**
 * 登记付款冲正草稿。过账只由最终通过动作内部消费，不得调用 `/post`。
 *
 * 页面主路径走创建草稿 + 提交确认；本函数只创建绑定，不启动审批。
 *
 * @param input 冲正草稿创建所需字段。
 */
export async function reversePayment(
    input: ReversePaymentInput,
): Promise<FormalSubmitResult> {
    const cached = submitIdempotency.get(input.idempotencyKey)
    if (cached) return cached
    try {
        const ensured = await ensurePaymentReversalDraft({
            sourcePaymentId: input.paymentId,
            reason: input.reason,
            idempotencyKey: input.idempotencyKey,
        })
        if (ensured.status !== "succeeded") {
            return failedPayment(ensured.code, ensured.message)
        }
        const result: FormalSubmitResult = {
            status: "succeeded",
            title: "付款冲正草稿已创建",
            description: "已登记付款冲正草稿并绑定审批流程，请确认后提交。",
            reference: ensured.reversal.reversalNo,
            operationId: input.idempotencyKey,
            documentNo: ensured.reversal.reversalNo,
            approval: ensured.reversal.approval,
            subjectStatus: ensured.reversal.status,
        }
        submitIdempotency.set(input.idempotencyKey, result)
        return result
    } catch (err) {
        return {
            status: "failed",
            title: "冲正失败",
            description: errorMessage(err, "付款冲正失败"),
            errorCode: "HTTP_ERROR",
        }
    }
}
