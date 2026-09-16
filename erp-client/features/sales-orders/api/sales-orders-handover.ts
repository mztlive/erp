/**
 * S3-07 销售责任交接 HTTP API（queryFn / mutationFn 纯函数）。
 *
 * 后端域：sales_order 交接命令（显式目标 + 原因 + 幂等键）。
 * 失败统一抛 ApiError（@/lib/api）。
 */

import { apiGet, apiPost } from "@/lib/api"

export type BackendHandoverCandidate = {
    user_id: string
    display_name: string
    account: string
}

export type BackendHandoverResult = {
    sales_order_id: string
    sales_owner_user_id: string
    business_org_unit_id: string
    version: number
    transferred_acceptance_task_ids: string[]
    kept_approval_task_count: number
}

export type SalesHandoverCandidate = {
    userId: string
    displayName: string
    account: string
}

export type SalesHandoverResult = {
    salesOrderId: string
    salesOwnerUserId: string
    businessOrgUnitId: string
    version: number
    transferredAcceptanceTaskIds: string[]
    keptApprovalTaskCount: number
}

/** 查询当前销售单可交接的合格目标候选（只含有效且具备验收资格人员）。 */
export async function fetchSalesHandoverCandidates(
    salesOrderId: string,
): Promise<SalesHandoverCandidate[]> {
    const rows = await apiGet<BackendHandoverCandidate[]>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}/handover-candidates`,
    )
    return rows.map((row) => ({
        userId: row.user_id,
        displayName: row.display_name,
        account: row.account,
    }))
}

/**
 * 提交显式销售责任交接。
 *
 * @param input 交接输入；业务组织省略表示保留原组织，不随接收人部门变化。
 */
export async function submitSalesHandover(input: {
    salesOrderId: string
    expectedVersion: number
    targetOwnerUserId: string
    targetBusinessOrgUnitId?: string
    reason: string
    idempotencyKey: string
}): Promise<SalesHandoverResult> {
    const result = await apiPost<BackendHandoverResult>(
        `/admin/sales-orders/${encodeURIComponent(input.salesOrderId)}/handover`,
        {
            expected_version: input.expectedVersion,
            target_owner_user_id: input.targetOwnerUserId,
            target_business_org_unit_id:
                input.targetBusinessOrgUnitId || undefined,
            reason: input.reason.trim(),
            idempotency_key: input.idempotencyKey,
        },
    )
    return {
        salesOrderId: result.sales_order_id,
        salesOwnerUserId: result.sales_owner_user_id,
        businessOrgUnitId: result.business_org_unit_id,
        version: result.version,
        transferredAcceptanceTaskIds:
            result.transferred_acceptance_task_ids ?? [],
        keptApprovalTaskCount: result.kept_approval_task_count ?? 0,
    }
}
