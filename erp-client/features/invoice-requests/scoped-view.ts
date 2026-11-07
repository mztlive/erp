/** 开票申请范围列表 HTTP（M08）：负责销售/申请人/当前开票处理人分别查询。 */

import { apiGet } from "@/lib/api"
import { nullIfNotFound } from "@/lib/api/errors"
import type { Page } from "@/lib/api/paging"
import { scopePageMeta } from "@/lib/funds-scope"

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
    >("/admin/sales-invoice-requests", toQuery(query))
    return {
        requests: page.items ?? [],
        total: page.total ?? 0,
        ...scopePageMeta(page),
    }
}

/** 独立详情重新解析详情动作；不可见与不存在统一为 null。 */
export async function fetchInvoiceRequestScopeDetail(
    id: string,
): Promise<ScopedInvoiceRequestWire | null> {
    return nullIfNotFound(async () => {
        const result = await apiGet<
            FundsScopedInvoiceRequestResultWire &
                Partial<ScopedInvoiceRequestWire>
        >(`/admin/sales-invoice-requests/${encodeURIComponent(id)}`)
        // 范围结果把行字段拍平在信封 data 上，没有再套一层 data。
        if (typeof result?.id === "string" && result.request_no) {
            return result as ScopedInvoiceRequestWire
        }
        return result?.data ?? null
    })
}

export function toQuery(query: InvoiceRequestScopeQuery) {
    return {
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
    }
}
