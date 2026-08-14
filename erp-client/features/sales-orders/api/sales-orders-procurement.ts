/**
 * W05 采购驳回改价与三路处置（queryFn / mutationFn 纯函数）。
 *
 * 后端域：sales_order。失败统一抛 ApiError（@/lib/api）。
 */

import { apiGet, apiPost, apiPut } from "@/lib/api"
import type {
    BackendProcurementRejectionResolutionResult,
    BackendSalesOrderDetail,
    ProcurementResolutionOutcome,
} from "@/features/sales-orders/api/contracts"
import { throwValidation } from "@/features/sales-orders/api/mappers"

export async function adjustProcurementRejectionDraft(input: {
    salesOrderId: string
    unitPriceGross: string
    note: string
}): Promise<{ ok: true }> {
    const detail = await apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${input.salesOrderId}`,
    )
    const wc = detail.working_copy
    if (!wc) {
        throwValidation("当前销售单无可用草稿，无法改价")
    }

    const lines = (wc.lines ?? []).map((line, index) => {
        const isVoucher = line.line_type === "VOUCHER"
        const unitPrice =
            index === 0
                ? input.unitPriceGross
                : (line.unit_price_gross ?? "0.0000")
        const base: Record<string, unknown> = {
            line_no: line.line_no,
            line_type: line.line_type,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot,
            spec_snapshot: line.spec_snapshot ?? null,
            unit_snapshot: line.unit_snapshot ?? null,
            goods: null,
            voucher: null,
        }
        if (isVoucher) {
            const cardCount = line.card_count ?? 1
            const face = line.face_value ?? "0.00"
            const faceTotal = (Number(face) * cardCount).toFixed(2)
            const txn = (Number(unitPrice) * cardCount).toFixed(2)
            const gift = (Number(faceTotal) - Number(txn)).toFixed(2)
            base.voucher = {
                face_value: face,
                card_count: cardCount,
                unit_price_gross: unitPrice,
                face_value_total: faceTotal,
                transaction_amount: txn,
                gift_amount: gift,
                gift_rate: null,
                card_form: line.card_form ?? "ELECTRONIC",
            }
        } else {
            const skuId = line.sku_id?.trim()
            const skuRevisionId = line.sku_revision_id?.trim()
            if (!skuId || !skuRevisionId) {
                throwValidation(
                    "历史草稿缺少精确 SKU 修订，请重新从公司商品池选择商品",
                )
            }
            base.goods = {
                sku_id: skuId,
                sku_revision_id: skuRevisionId,
                welfare_scenario: null,
                fulfillment_mode: "COMPANY_WAREHOUSE",
                fulfillment_due_at: Math.floor(Date.now() / 1000),
                quantity: line.quantity ?? "0",
                base_unit_code:
                    line.base_unit_code ?? line.unit_snapshot ?? "EA",
                unit_price_gross: unitPrice,
            }
        }
        return base
    })

    await apiPut(`/admin/sales-orders/${input.salesOrderId}/working-copy`, {
        version: detail.version,
        draft: {
            editor_user_id: wc.editor_user_id,
            customer_name: "", // 后端 Save 会用草稿覆盖；名称由服务端实体保留时可能校验
            contract_no: null,
            settlement_party_name: null,
            payment_term_code: "CUSTOM",
            payment_term_name: "合同约定",
            invoice_type: "SPECIAL",
            tax_point: "0",
            project_name: null,
            business_remark: input.note || null,
            voucher_category_sku_id: null,
            voucher_expiry_at: null,
            lines,
        },
    })

    return { ok: true }
}

export type ResolveProcurementRejectionIntent = {
    salesOrderId: string
} & (
    | {
          action: "RESUBMIT_CHANGED_TERMS"
          customerReconfirmationEvidenceIds: string[]
      }
    | {
          action: "REQUEST_LOW_MARGIN_ACCEPTANCE"
          lowMarginAcceptanceReason: string
          evidenceReferenceIds: string[]
      }
    | {
          action: "VOID_AFTER_REJECTION"
          voidReasonCode: string
          comment: string
      }
)

export type ResolveProcurementRejectionPayload =
    ResolveProcurementRejectionIntent & {
        rejectedProcurementConfirmationId: string
        rejectedSubmissionId: string
        expectedSalesOrderLockVersion: number
        expectedDraftVersion?: number
    }

export type ResolveProcurementRejectionInput =
    ResolveProcurementRejectionPayload & {
        idempotencyKey: string
    }

/**
 * 冻结采购驳回处置所需的对象身份与版本。调用方必须把返回值和命令键一起保存；
 * 结果未知后的重试不得重新读取并拼装另一份命令。
 */
export async function prepareProcurementRejectionResolution(
    intent: ResolveProcurementRejectionIntent,
): Promise<ResolveProcurementRejectionPayload> {
    const detail = await apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${intent.salesOrderId}`,
    )
    const rejection = detail.open_procurement_rejection
    if (!rejection) {
        throwValidation("当前销售单没有可处理的采购驳回")
    }
    if (intent.action !== "VOID_AFTER_REJECTION" && !detail.working_copy) {
        throwValidation("当前销售单没有可处理的工作副本")
    }
    return {
        ...intent,
        rejectedProcurementConfirmationId:
            rejection.procurement_confirmation_id,
        rejectedSubmissionId: rejection.submission_id,
        expectedSalesOrderLockVersion: detail.version,
        expectedDraftVersion: detail.working_copy?.version,
    }
}

/**
 * 处置采购驳回的唯一入口。对象身份和版本必须来自同一次 prepare 并被冻结；
 * 客户证据只能由调用方显式提供，禁止补造引用或结果编号。
 */
export async function resolveProcurementRejection(
    input: ResolveProcurementRejectionInput,
): Promise<ProcurementResolutionOutcome> {
    const common = {
        action: input.action,
        sales_order_id: input.salesOrderId,
        rejected_procurement_confirmation_id:
            input.rejectedProcurementConfirmationId,
        rejected_submission_id: input.rejectedSubmissionId,
        expected_sales_order_lock_version: input.expectedSalesOrderLockVersion,
        operation_id: input.idempotencyKey,
        idempotency_key: input.idempotencyKey,
    }
    let command: Record<string, unknown>
    if (input.action === "RESUBMIT_CHANGED_TERMS") {
        if (input.customerReconfirmationEvidenceIds.length === 0) {
            throwValidation("改品或改价重提必须登记客户重新确认依据")
        }
        command = {
            ...common,
            expected_draft_version: input.expectedDraftVersion,
            customer_reconfirmation_evidence_ids:
                input.customerReconfirmationEvidenceIds,
        }
    } else if (input.action === "REQUEST_LOW_MARGIN_ACCEPTANCE") {
        if (!input.lowMarginAcceptanceReason.trim()) {
            throwValidation("请填写低毛利承接理由")
        }
        if (input.evidenceReferenceIds.length === 0) {
            throwValidation("申请低毛利承接必须登记证据依据")
        }
        command = {
            ...common,
            expected_draft_version: input.expectedDraftVersion,
            low_margin_acceptance_reason:
                input.lowMarginAcceptanceReason.trim(),
            evidence_reference_ids: input.evidenceReferenceIds,
        }
    } else {
        if (!input.voidReasonCode.trim() || !input.comment.trim()) {
            throwValidation("作废原因代码和说明不能为空")
        }
        command = {
            ...common,
            void_reason_code: input.voidReasonCode.trim(),
            comment: input.comment.trim(),
        }
    }

    const result = await apiPost<BackendProcurementRejectionResolutionResult>(
        `/admin/sales-orders/${input.salesOrderId}/procurement-rejection-resolution`,
        command,
    )
    if (result.outcome === "CHANGED_TERMS_RESUBMITTED") {
        return {
            outcome: result.outcome,
            reference: result.new_procurement_confirmation_id,
            detail: "已冻结新提交并创建新的采购确认待办；旧驳回记录保持历史。",
            newSubmissionNo: result.new_submission_no,
            newSubjectHash: result.new_submission_id,
            newWorkItemId: result.new_procurement_work_item_id,
            reviewStatus: "RESOLVED",
            primaryStatusLabel: "待二次确认",
        }
    }
    if (result.outcome === "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED") {
        return {
            outcome: result.outcome,
            reference: result.low_margin_confirmation_id,
            detail: "已冻结原商业条件并转交销售上级确认低毛利承接。",
            newSubmissionNo: result.new_submission_no,
            newSubjectHash: result.new_submission_id,
            newWorkItemId: result.low_margin_manager_work_item_id,
            reviewStatus: "RESOLVED",
            primaryStatusLabel: "待销售上级确认",
        }
    }
    return {
        outcome: result.outcome,
        reference: result.workflow_action_id,
        detail: "销售单已作废，采购驳回与历史提交记录已保留。",
        reviewStatus: "VOIDED",
        primaryStatusLabel: "已作废",
    }
}
