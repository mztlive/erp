/**
 * W27 API 供应商结算 · 结算单列表查询
 * 从 api/settlements.ts 拆出；列表筛选语义（客户端兜底过滤等）保持不变。
 */

import { apiGet } from "@/lib/api"
import type {
    DifferenceType,
    SettlementListView,
    SettlementView,
} from "@/features/supplier-settlements/types"
import { VIEW_LABEL } from "@/features/supplier-settlements/types"
import {
    asStatus,
    toListRow,
    type BackendStatementPage,
} from "@/features/supplier-settlements/api/settlements-wire"

export type ListQueryInput = {
    view: SettlementView
    supplierId?: string
    periodFrom?: string
    periodTo?: string
    status?: string
    differenceType?: DifferenceType
    q?: string
    page: number
    pageSize?: number
}

export async function fetchSettlementList(
    input: ListQueryInput,
): Promise<SettlementListView> {
    const queriedAt = new Date().toISOString()
    const pageSize = input.pageSize ?? 50
    // Map view → status filter when possible
    let statusFilter = input.status
    if (!statusFilter) {
        if (input.view === "confirmed") statusFilter = "CONFIRMED"
        else if (input.view === "pending") statusFilter = undefined
    }

    const pageRes = await apiGet<BackendStatementPage>(
        "/admin/supplier-settlement-statements",
        {
            page: input.page,
            page_size: pageSize,
            supplier_id: input.supplierId,
            status: statusFilter?.split(",")[0]?.trim() || undefined,
            period_from: input.periodFrom,
            period_to: input.periodTo,
            statement_no: input.q?.trim() || undefined,
            sort_by: "period_end",
            sort_dir: "asc",
        },
    )

    let statements = pageRes.items ?? []

    // Client-side view filters not supported by backend
    if (input.view === "pending") {
        statements = statements.filter((s) => {
            const st = asStatus(s.status)
            return (
                st === "DRAFT" ||
                st === "PENDING_RECONCILE" ||
                st === "HAS_DIFFERENCE" ||
                st === "PENDING_REVIEW"
            )
        })
    }

    const rows = statements.map(toListRow)
    const total = pageRes.total ?? rows.length
    const suppliersMap = new Map<string, string>()
    for (const s of statements) suppliersMap.set(s.supplier_id, s.supplier_id)

    const filterParts = [
        input.view !== "pending" ? `视图=${VIEW_LABEL[input.view]}` : null,
        input.supplierId ? `供应商=${input.supplierId}` : null,
        input.periodFrom || input.periodTo
            ? `期间=${input.periodFrom ?? "…"} ~ ${input.periodTo ?? "…"}`
            : null,
        input.q ? `搜索=${input.q}` : null,
    ].filter(Boolean)

    return {
        view: input.view,
        rows,
        page: pageRes.page ?? input.page,
        pageSize: pageRes.page_size ?? pageSize,
        total,
        totals: {
            pendingReconcile: pageRes.stats.pending_reconciliation_count,
            hasDifference: pageRes.stats.has_difference_count,
            pendingReview: pageRes.stats.pending_review_count,
            confirmedAmountThisPeriod: String(pageRes.stats.confirmed_amount),
        },
        metrics: {
            pending: pageRes.stats.pending_reconciliation_count,
            hasDifference: pageRes.stats.has_difference_count,
            pendingReview: pageRes.stats.pending_review_count,
            confirmedAmount: String(pageRes.stats.confirmed_amount),
        },
        suppliers: Array.from(suppliersMap.entries()).map(
            ([supplierId, supplierName]) => ({ supplierId, supplierName }),
        ),
        emptyReason: total === 0 ? "NO_STATEMENTS" : undefined,
        hasModulePermission: true,
        hasDataScope: true,
        permissionVersion: "server",
        sourceAsOf: queriedAt,
        queriedAt,
        filterSummary: filterParts.length
            ? filterParts.join(" · ")
            : "默认待处理视图",
    }
}
