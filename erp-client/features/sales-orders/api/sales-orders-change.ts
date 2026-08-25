/**
 * W05 销售变更单创建与复核决定（queryFn / mutationFn 纯函数）。
 *
 * 后端域：sales_change_order。失败统一抛 ApiError（@/lib/api）。
 */

import { apiGet, apiPost } from "@/lib/api"
import type { BackendSalesChangeOrder } from "@/features/sales-orders/api/contracts"
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

export type StartSalesChangeOrderPayload = StartSalesChangeOrderIntent

export type StartSalesChangeOrderInput = StartSalesChangeOrderPayload & {
    idempotencyKey: string
}

/** 创建改单；完整工作副本由后端从当前生效版本派生。 */
export async function startSalesChangeOrder(
    input: StartSalesChangeOrderInput,
): Promise<SalesChangeOrderSummary> {
    if (
        !Number.isSafeInteger(input.baseRevisionNo) ||
        input.baseRevisionNo <= 0
    ) {
        throwValidation("销售单尚无可变更的生效版本")
    }
    const created = await apiPost<BackendSalesChangeOrder>(
        "/admin/sales-change-orders",
        {
            sales_order_id: input.salesOrderId,
            expected_base_revision_no: input.baseRevisionNo,
            change_type: input.nature === "card_voucher" ? "OTHER" : "AMOUNT",
            reason: "销售发起变更",
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
