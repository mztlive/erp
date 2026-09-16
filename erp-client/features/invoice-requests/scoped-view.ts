/** 开票申请范围列表 HTTP（M08）：负责销售/申请人/当前开票处理人分别查询。 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import type {
    FundsScopedInvoiceRequestPageWire,
    FundsScopedInvoiceRequestResultWire,
    InvoiceRequestScopeListView,
    InvoiceRequestScopeQuery,
    ScopedInvoiceRequestWire,
} from "./scoped"

/** 按当前生效条件分页读取范围行；跨页必须携带首个响应的范围版本。 */
export async function fetchInvoiceRequestScopeList(
    query: InvoiceRequestScopeQuery,
): Promise<InvoiceRequestScopeListView> {
    const page = await apiGet<
        Page<ScopedInvoiceRequestWire> & FundsScopedInvoiceRequestPageWire
    >("/admin/sales-invoice-requests", {
        page: query.page,
        page_size: query.pageSize,
        scope_version: query.scopeVersion,
        sales_owner_user_ids: query.salesOwnerUserIds || undefined,
        applicant_user_ids: query.applicantUserIds || undefined,
        handler_user_ids: query.handlerUserIds || undefined,
        org_unit_ids: query.orgUnitIds || undefined,
        include_descendants:
            query.orgUnitIds && query.includeDescendants ? true : undefined,
        q: query.q?.trim() || undefined,
        status: query.status,
        sales_order_id: query.salesOrderId,
        customer_id: query.customerId,
        receivable_account_id: query.receivableAccountId,
    })
    return {
        requests: page.items ?? [],
        total: page.total ?? 0,
        summary: page.summary,
        ownerOptions: page.owner_options ?? [],
        scopeVersion: page.scope_version,
        policyVersion: page.policy_version,
        organizationVersion: page.organization_version,
        scopeSummary: page.scope_summary,
        asOf: page.as_of,
        emptyReason: page.empty_reason,
        hasScope: page.empty_reason !== "no_scope",
    }
}

/** 独立详情重新解析详情动作；不可见与不存在统一为 null。 */
export async function fetchInvoiceRequestScopeDetail(
    id: string,
): Promise<ScopedInvoiceRequestWire | null> {
    try {
        const result = await apiGet<FundsScopedInvoiceRequestResultWire>(
            `/admin/sales-invoice-requests/${encodeURIComponent(id)}`,
        )
        return result.data
    } catch (error) {
        if (
            typeof error === "object" &&
            error !== null &&
            "status" in error &&
            error.status === 404
        )
            return null
        throw error
    }
}
