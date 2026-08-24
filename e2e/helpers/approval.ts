import type { APIRequestContext } from "@playwright/test"

import { api, apiLogin, apiProfile } from "./api"

/**
 * 为当前测试发布“采购确认 → 销售领导审批”的销售单流程新版本。
 * reset 后的通用脚本先发布默认版本，本 helper 从当前发布版复制草稿，避免影响其他 flow。
 */
export async function publishTwoStepSalesOrderApproval(
    request: APIRequestContext,
): Promise<void> {
    const [adminToken, procurementToken, salesLeaderToken] = await Promise.all([
        apiLogin(request, "admin"),
        apiLogin(request, "procurement"),
        apiLogin(request, "salesLeader"),
    ])
    const [procurement, salesLeader] = await Promise.all([
        apiProfile(request, procurementToken),
        apiProfile(request, salesLeaderToken),
    ])
    const created = await api<{
        definition_id: string
        definition_lock_version: string | number
    }>(request, "POST", "/admin/approval-process-definitions/drafts", {
        token: adminToken,
        body: {
            document_type: "sales_order",
            name: "销售单两级审批（采购建单 E2E）",
            draft_source: "CURRENT_PUBLISHED",
            idempotency_key: `e2e-sales-order-two-step-${Date.now()}`,
        },
    })
    const updated = await api<{ definition_lock_version: string | number }>(
        request,
        "PUT",
        `/admin/approval-process-definitions/${encodeURIComponent(created.definition_id)}/nodes`,
        {
            token: adminToken,
            body: {
                expected_definition_lock_version: String(
                    created.definition_lock_version,
                ),
                nodes: [
                    {
                        node_name: "采购确认",
                        display_order: 1,
                        assignee_user_id: procurement.userid,
                    },
                    {
                        node_name: "销售领导审批",
                        display_order: 2,
                        assignee_user_id: salesLeader.userid,
                    },
                ],
            },
        },
    )
    await api(
        request,
        "POST",
        `/admin/approval-process-definitions/${encodeURIComponent(created.definition_id)}/publish`,
        {
            token: adminToken,
            body: {
                expected_definition_lock_version: String(
                    updated.definition_lock_version,
                ),
                idempotency_key: `e2e-sales-order-two-step-publish-${Date.now()}`,
            },
        },
    )
}
