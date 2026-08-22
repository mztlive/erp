/**
 * W05 销售变更单创建与复核决定（queryFn / mutationFn 纯函数）。
 *
 * 后端域：sales_change_order。失败统一抛 ApiError（@/lib/api）。
 */

import { apiGet, apiPost } from "@/lib/api"
import type {
    BackendContractDetail,
    BackendSalesChangeOrder,
    BackendSalesOrderDetail,
} from "@/features/sales-orders/api/contracts"
import {
    mapChangeOrder,
    throwValidation,
} from "@/features/sales-orders/api/mappers"
import type {
    SalesChangeOrderSummary,
    SalesOrderNature,
} from "@/features/sales-orders/types"

export type StartSalesChangeOrderIntent = {
    salesOrderId: string
    baseRevisionNo: number
    nature: "physical_service" | "card_voucher"
}

export type StartSalesChangeOrderPayload = StartSalesChangeOrderIntent & {
    command: Record<string, unknown>
}

export type StartSalesChangeOrderInput = StartSalesChangeOrderPayload & {
    idempotencyKey: string
}

/** 冻结改单完整载荷；正式请求结果未知后禁止重新从可变详情拼装。 */
export async function prepareStartSalesChangeOrder(
    input: StartSalesChangeOrderIntent,
): Promise<StartSalesChangeOrderPayload> {
    const detail = await apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${input.salesOrderId}`,
    )
    const wc = detail.working_copy
    const latestRev = detail.revisions?.[0]
    // 已生效订单的 FIRST_SUBMISSION 工作副本已提交（status=SUBMITTED），
    // 详情视图只返回可编辑副本（Editing/Conflict），故 wc 通常为空；
    // 与 fetchSalesOrderDraftForResume 同策略：回退到最近一次提交快照。
    const submissions = [...(detail.submissions ?? [])].sort(
        (a, b) => (b.submission_no ?? 0) - (a.submission_no ?? 0),
    )
    const latestSubmission = submissions[0]
    const draftSource =
        wc && wc.lines.length > 0 ? wc : latestSubmission

    // 变更单创建需要完整 draft；以当前工作副本/最近提交快照行作为目标草稿骨架。
    // 字段不足时后端会校验失败并经 ApiError 抛出。
    let contractNo: string | null = null
    let customerName = detail.customer_id
    let settlementName: string | null = null
    let paymentCode = "CUSTOM"
    let paymentName = "合同约定"
    let invoiceType = "SPECIAL"
    let taxPoint = "0"

    if (detail.contract_id) {
        try {
            const contract = await apiGet<BackendContractDetail>(
                `/admin/contracts/${detail.contract_id}`,
            )
            contractNo = contract.contract_no
            const rev = contract.revisions.find(
                (r) => r.id === contract.current_revision_id,
            )
            if (rev) {
                customerName = rev.customer_name
                settlementName = rev.settlement_party_name
                paymentCode = rev.payment_term_code
                paymentName = rev.payment_term_name
                invoiceType = rev.invoice_type
                taxPoint = rev.tax_point
            }
        } catch {
            // ignore
        }
    }

    const lines =
        draftSource?.lines?.map((line) => {
            const isVoucher = line.line_type === "VOUCHER"
            const row: Record<string, unknown> = {
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
                const unitPrice = line.unit_price_gross ?? "0.0000"
                const faceTotal = (Number(face) * cardCount).toFixed(2)
                const txn = (Number(unitPrice) * cardCount).toFixed(2)
                const gift = (Number(faceTotal) - Number(txn)).toFixed(2)
                row.voucher = {
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
                row.goods = {
                    sku_id: skuId,
                    sku_revision_id: skuRevisionId,
                    welfare_scenario: null,
                    fulfillment_mode: "COMPANY_WAREHOUSE",
                    fulfillment_due_at: Math.floor(Date.now() / 1000),
                    quantity: line.quantity ?? "0",
                    base_unit_code: line.base_unit_code ?? "EA",
                    unit_price_gross: line.unit_price_gross ?? "0.0000",
                }
            }
            return row
        }) ?? []

    if (lines.length === 0) {
        throwValidation("无法发起变更：缺少可变更的明细草稿")
    }

    return {
        ...input,
        baseRevisionNo: input.baseRevisionNo || latestRev?.revision_no || 0,
        command: {
            sales_order_id: input.salesOrderId,
            change_type: input.nature === "card_voucher" ? "OTHER" : "AMOUNT",
            reason: "销售发起变更",
            draft: {
                editor_user_id:
                    (wc && wc.lines.length > 0
                        ? wc.editor_user_id
                        : latestSubmission?.submitted_by) ?? "unknown",
                customer_name: customerName,
                contract_no: contractNo,
                settlement_party_name: settlementName,
                payment_term_code: paymentCode,
                payment_term_name: paymentName,
                invoice_type: invoiceType,
                tax_point: taxPoint,
                project_name: null,
                business_remark: null,
                voucher_category_sku_id: null,
                voucher_expiry_at: null,
                lines,
            },
        },
    }
}

/** 使用已冻结载荷创建改单。 */
export async function startSalesChangeOrder(
    input: StartSalesChangeOrderInput,
): Promise<SalesChangeOrderSummary> {
    const created = await apiPost<BackendSalesChangeOrder>(
        "/admin/sales-change-orders",
        {
            ...input.command,
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        ...mapChangeOrder(created, input.nature),
        baseRevisionNo: input.baseRevisionNo,
    }
}

/**
 * 读取销售变更单详情，补齐统一只读审批投影。
 *
 * @param id 变更单 ID。
 * @param nature 原销售单业务性质，仅用于兼容旧摘要字段。
 */
export async function fetchSalesChangeOrderDetail(
    id: string,
    nature: SalesOrderNature,
): Promise<SalesChangeOrderSummary> {
    const detail = await apiGet<BackendSalesChangeOrder>(
        `/admin/sales-change-orders/${encodeURIComponent(id)}`,
    )
    return mapChangeOrder(detail, nature)
}

export type SubmitSalesChangeOrderInput = Readonly<{
    salesChangeOrderId: string
    salesOrderId: string
    version: number
    nature: SalesOrderNature
    idempotencyKey: string
}>

/**
 * 提交销售变更并启动统一审批。客户端不得选择定义或审批人。
 *
 * @param input 期望版本、幂等键与业务性质。
 */
export async function submitSalesChangeOrder(
    input: SubmitSalesChangeOrderInput,
): Promise<SalesChangeOrderSummary> {
    const submitted = await apiPost<BackendSalesChangeOrder>(
        `/admin/sales-change-orders/${encodeURIComponent(input.salesChangeOrderId)}/submit-impact`,
        {
            version: input.version,
            idempotency_key: input.idempotencyKey,
        },
    )
    return mapChangeOrder(submitted, input.nature)
}

export type SalesChangeReviewDecisionInput = Readonly<{
    salesChangeOrderId: string
    handlerKey: "sales_change_impact_review" | "sales_change_finance_review"
    decision: "APPROVE" | "REJECT"
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    decisionReason?: string
    idempotencyKey: string
}>

/** 提交销售变更复核强命令；任务处理器与决定共同固定唯一业务端点。 */
export async function submitSalesChangeReviewDecision(
    input: SalesChangeReviewDecisionInput,
): Promise<BackendSalesChangeOrder> {
    const taskVersion = Number(input.expectedTaskVersion)
    if (!Number.isSafeInteger(taskVersion) || taskVersion <= 0) {
        throwValidation("待办版本无效，请刷新任务后重试")
    }
    const action =
        input.handlerKey === "sales_change_impact_review"
            ? input.decision === "APPROVE"
                ? "impact-confirm"
                : "impact-reject"
            : input.decision === "APPROVE"
              ? "finance-confirm"
              : "finance-reject"
    return apiPost<BackendSalesChangeOrder>(
        `/admin/sales-change-orders/${encodeURIComponent(input.salesChangeOrderId)}/${action}`,
        {
            work_item_id: input.workItemId,
            expected_task_version: taskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            decision_reason: input.decisionReason?.trim() || null,
            idempotency_key: input.idempotencyKey,
        },
    )
}
