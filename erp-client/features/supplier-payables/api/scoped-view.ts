/** 供应商往来范围列表 HTTP（M09）：采购负责人/付款/收票经办人分别查询。 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import type {
    FundsScopedPayablePageWire,
    FundsScopedPayableResultWire,
    FundsScopedPaymentPageWire,
    FundsScopedPaymentResultWire,
    FundsScopedPurchaseAllocationPageWire,
    ScopedPayableAccountWire,
    ScopedPurchaseInvoiceAllocationWire,
    ScopedSupplierPaymentWire,
    SupplierScopeDetail,
    SupplierScopeListView,
    SupplierScopeQuery,
} from "./scoped"

function baseQuery(query: SupplierScopeQuery): Record<string, unknown> {
    return {
        page: query.page,
        page_size: query.pageSize,
        scope_version: query.scopeVersion,
        procurement_owner_user_ids: query.procurementOwnerUserIds || undefined,
        operator_user_ids: query.operatorUserIds || undefined,
        org_unit_ids: query.orgUnitIds || undefined,
        include_descendants:
            query.orgUnitIds && query.includeDescendants ? true : undefined,
        q: query.q?.trim() || undefined,
    }
}

/** 按当前生效条件分页读取范围行；跨页必须携带首个响应的范围版本。 */
export async function fetchSupplierScopeList(
    query: SupplierScopeQuery,
): Promise<SupplierScopeListView> {
    if (query.view === "payable") {
        const page = await apiGet<
            Page<ScopedPayableAccountWire> & FundsScopedPayablePageWire
        >("/admin/payable-accounts", {
            ...baseQuery(query),
            source_document_id: query.purchaseOrderId,
            supplier_id: query.supplierId,
            sort_by: "created_at",
            sort_dir: "desc",
        })
        return {
            view: query.view,
            payables: page.items ?? [],
            payments: [],
            allocations: [],
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
    if (query.view === "payment") {
        const page = await apiGet<
            Page<ScopedSupplierPaymentWire> & FundsScopedPaymentPageWire
        >("/admin/supplier-payments", {
            ...baseQuery(query),
            supplier_id: query.supplierId,
            sort_by: "paid_at",
            sort_dir: "desc",
        })
        return {
            view: query.view,
            payables: [],
            payments: page.items ?? [],
            allocations: [],
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
    const page = await apiGet<
        Page<ScopedPurchaseInvoiceAllocationWire> &
            FundsScopedPurchaseAllocationPageWire
    >("/admin/purchase-invoice-allocations", {
        ...baseQuery(query),
        payable_account_id: query.payableAccountId,
        sort_by: "created_at",
        sort_dir: "desc",
    })
    return {
        view: query.view,
        payables: [],
        payments: [],
        allocations: page.items ?? [],
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
export async function fetchSupplierScopeDetail(
    kind: "payable" | "payment",
    id: string,
): Promise<SupplierScopeDetail | null> {
    try {
        if (kind === "payable") {
            const result = await apiGet<FundsScopedPayableResultWire>(
                `/admin/payable-accounts/${encodeURIComponent(id)}`,
            )
            return { kind, payable: result.data }
        }
        const result = await apiGet<FundsScopedPaymentResultWire>(
            `/admin/supplier-payments/${encodeURIComponent(id)}`,
        )
        return { kind, payment: result.data }
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
