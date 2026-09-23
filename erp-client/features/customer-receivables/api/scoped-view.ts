/** 客户往来范围列表 HTTP（M07/M08）：各视图独立分页，同一对象映射裁剪。 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import type {
    FundsScopedInvoicePageWire,
    FundsScopedInvoiceResultWire,
    FundsScopedReceiptPageWire,
    FundsScopedReceiptResultWire,
    FundsScopedReceivablePageWire,
    FundsScopedReceivableResultWire,
    ReceivableScopeDetail,
    ReceivableScopeListView,
    ReceivableScopeQuery,
    ScopedCustomerReceiptWire,
    ScopedInvoiceWire,
    ScopedReceivableAccountWire,
} from "./scoped"

function toQuery(query: ReceivableScopeQuery): Record<string, unknown> {
    return {
        page: query.page,
        page_size: query.pageSize,
        scope_version: query.scopeVersion,
        sales_owner_user_ids: query.salesOwnerUserIds || undefined,
        operator_user_ids: query.operatorUserIds || undefined,
        operator_kind:
            query.view === "receipt" ? query.operatorKind : undefined,
        org_unit_ids: query.orgUnitIds || undefined,
        include_descendants:
            query.orgUnitIds && query.includeDescendants ? true : undefined,
        q: query.q?.trim() || undefined,
        status: query.status,
        counterparty_party_id: query.counterpartyPartyId,
        customer_id: query.customerId,
        sales_order_id: query.salesOrderId,
        receivable_account_id: query.receivableAccountId,
        invoice_direction: query.view === "sales_invoice" ? "sales" : undefined,
        sort_by:
            query.view === "receivable"
                ? "created_at"
                : query.view === "receipt"
                  ? "received_at"
                  : "invoice_date",
        sort_dir: "desc",
    }
}

async function loadScopedView(
    query: ReceivableScopeQuery,
): Promise<ReceivableScopeListView> {
    if (query.view === "receivable") {
        const page = await apiGet<
            Page<ScopedReceivableAccountWire> & FundsScopedReceivablePageWire
        >("/admin/receivable-accounts", toQuery(query))
        return {
            view: query.view,
            receivables: page.items ?? [],
            receipts: [],
            invoices: [],
            total: page.total ?? 0,
            summary: page.summary,
            scopeVersion: page.scope_version,
            policyVersion: page.policy_version,
            organizationVersion: page.organization_version,
            scopeSummary: page.scope_summary,
            asOf: page.as_of,
            emptyReason: page.empty_reason,
            hasScope: page.empty_reason !== "no_scope",
        }
    }
    if (query.view === "receipt") {
        const page = await apiGet<
            Page<ScopedCustomerReceiptWire> & FundsScopedReceiptPageWire
        >("/admin/customer-receipts", toQuery(query))
        return {
            view: query.view,
            receivables: [],
            receipts: page.items ?? [],
            invoices: [],
            total: page.total ?? 0,
            summary: page.summary,
            scopeVersion: page.scope_version,
            policyVersion: page.policy_version,
            organizationVersion: page.organization_version,
            scopeSummary: page.scope_summary,
            asOf: page.as_of,
            emptyReason: page.empty_reason,
            hasScope: page.empty_reason !== "no_scope",
        }
    }
    const page = await apiGet<
        Page<ScopedInvoiceWire> & FundsScopedInvoicePageWire
    >("/admin/invoices", toQuery(query))
    const invoices = (page.items ?? []).filter(
        (item) => item.invoice_direction === "sales" || !item.invoice_direction,
    )
    return {
        view: query.view,
        receivables: [],
        receipts: [],
        invoices,
        total: page.total ?? invoices.length,
        summary: page.summary,
        scopeVersion: page.scope_version,
        policyVersion: page.policy_version,
        organizationVersion: page.organization_version,
        scopeSummary: page.scope_summary,
        asOf: page.as_of,
        emptyReason: page.empty_reason,
        hasScope: page.empty_reason !== "no_scope",
    }
}

/** 按当前生效条件分页读取范围行；跨页必须携带首个响应的范围版本。 */
export async function fetchReceivableScopeList(
    query: ReceivableScopeQuery,
): Promise<ReceivableScopeListView> {
    return loadScopedView(query)
}

/** 独立详情重新解析详情动作；不可见与不存在统一为 null。 */
export async function fetchReceivableScopeDetail(
    kind: "receivable" | "receipt" | "invoice",
    id: string,
): Promise<ReceivableScopeDetail | null> {
    try {
        if (kind === "receivable") {
            const result = await apiGet<FundsScopedReceivableResultWire>(
                `/admin/receivable-accounts/${encodeURIComponent(id)}`,
            )
            return { kind, receivable: result.data }
        }
        if (kind === "receipt") {
            const result = await apiGet<FundsScopedReceiptResultWire>(
                `/admin/customer-receipts/${encodeURIComponent(id)}`,
            )
            return { kind, receipt: result.data }
        }
        const result = await apiGet<FundsScopedInvoiceResultWire>(
            `/admin/invoices/${encodeURIComponent(id)}`,
        )
        return { kind, invoice: result.data }
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
