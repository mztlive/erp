/**
 * W05 销售变更单创建与复核决定（queryFn / mutationFn 纯函数）。
 *
 * 后端域：sales_change_order。失败统一抛 ApiError（@/lib/api）。
 */

import { isDataScopeChanged } from "@/features/data-scope/cache"
import { apiGet, apiPost } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import type {
    BackendSalesChangeOrder,
    PageView,
} from "@/features/sales-orders/api/contracts"
import {
    mapChangeOrder,
    throwValidation,
} from "@/features/sales-orders/api/mappers"
import type {
    SalesChangeOrderSummary,
    SalesOrderNature,
} from "@/features/sales-orders/types"

function isApiError(error: unknown): error is ApiError {
    return (
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        "message" in error
    )
}

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

export type ActiveSalesChangeOrderResult = {
    order: SalesChangeOrderSummary | null
    emptyReason?: string | null
    scopeVersion?: string
}

type SalesChangeOrderListPage = PageView<BackendSalesChangeOrder> & {
    empty_reason?: string | null
    scope_version?: string
}

const isOpenSalesChangeStatus = (status?: string): boolean =>
    status !== "EFFECTIVE" && status !== "VOIDED" && status !== "REJECTED"

/**
 * 把变更列表页的范围元数据收成前端字段；`no_scope` 一律空集。
 *
 * @param page 变更列表响应。
 * @param order 已按详情接口证明的在途变更；缺省表示无在途改单。
 */
function salesChangeListResult(
    page: SalesChangeOrderListPage,
    order: SalesChangeOrderSummary | null = null,
): ActiveSalesChangeOrderResult {
    return {
        order: page.empty_reason === "no_scope" ? null : order,
        emptyReason: page.empty_reason,
        scopeVersion: page.scope_version,
    }
}

/**
 * 读取原销售单上尚未终态的销售变更，并补详情审批投影。
 *
 * @param salesOrderId 原销售单 ID。
 * @param nature 原销售单业务性质，仅用于兼容旧摘要字段。
 * @param scopeVersion 跨页必须回传的范围版本。
 */
export async function fetchActiveSalesChangeOrder(
    salesOrderId: string,
    nature: SalesOrderNature,
    scopeVersion?: string,
): Promise<ActiveSalesChangeOrderResult> {
    const page = await apiGet<SalesChangeOrderListPage>(
        "/admin/sales-change-orders",
        {
            sales_order_id: salesOrderId,
            scope_version: scopeVersion,
            page: 1,
            page_size: 10,
        },
    )
    if (page.empty_reason === "no_scope") {
        return salesChangeListResult(page)
    }
    const active =
        (page.items ?? []).find(
            (change) =>
                change.sales_order_id === salesOrderId &&
                isOpenSalesChangeStatus(change.status),
        ) ?? null
    if (!active) return salesChangeListResult(page)
    try {
        const order = await fetchSalesChangeOrderDetail(
            active.id,
            nature,
            salesOrderId,
        )
        return salesChangeListResult(page, order)
    } catch (error) {
        if (isDataScopeChanged(error)) throw error
        if (isApiError(error) && error.status === 404) {
            return salesChangeListResult(page)
        }
        throw error
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
    expectedSalesOrderId?: string,
): Promise<SalesChangeOrderSummary> {
    const detail = await apiGet<BackendSalesChangeOrder>(
        `/admin/sales-change-orders/${encodeURIComponent(id)}`,
    )
    if (
        expectedSalesOrderId &&
        detail.sales_order_id !== expectedSalesOrderId
    ) {
        throwValidation("该变更单不属于当前销售单")
    }
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
