/**
 * W27 API 供应商结算 · 结算单列表查询
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
import { hasAppliedSettlementFilters } from "@/features/supplier-settlements/lib/settlement-list-filters"

export type ListQueryInput = {
    view: SettlementView
    supplierId?: string
    periodFrom?: string
    periodTo?: string
    status?: string
    differenceType?: DifferenceType
    q?: string
    ownerUserIds?: string
    operatorUserIds?: string
    handlerUserIds?: string
    orgUnitIds?: string
    includeDescendants?: boolean
    scopeVersion?: string
    currentUserId?: string
    page: number
    pageSize?: number
}

function ownerIds(input: ListQueryInput): string | undefined {
    if (input.ownerUserIds?.trim()) return input.ownerUserIds.trim()
    if (input.view === "prepared_by_me" && input.currentUserId?.trim()) {
        return input.currentUserId.trim()
    }
    return undefined
}

function handlerIds(input: ListQueryInput): string | undefined {
    if (input.handlerUserIds?.trim()) return input.handlerUserIds.trim()
    if (input.view === "review_by_me" && input.currentUserId?.trim()) {
        return input.currentUserId.trim()
    }
    return undefined
}

export async function fetchSettlementList(
    input: ListQueryInput,
): Promise<SettlementListView> {
    const queriedAt = new Date().toISOString()
    const pageSize = input.pageSize ?? 50
    let statusFilter = input.status
    if (!statusFilter && input.view === "confirmed") statusFilter = "CONFIRMED"

    const pageRes = await apiGet<BackendStatementPage>(
        "/admin/supplier-settlement-statements",
        {
            page: input.page,
            page_size: pageSize,
            supplier_id: input.supplierId,
            status: statusFilter?.trim() || undefined,
            period_from: input.periodFrom,
            period_to: input.periodTo,
            q: input.q?.trim() || undefined,
            owner_user_ids: ownerIds(input),
            operator_user_ids: input.operatorUserIds?.trim() || undefined,
            handler_user_ids: handlerIds(input),
            org_unit_ids: input.orgUnitIds?.trim() || undefined,
            include_descendants: input.includeDescendants || undefined,
            scope_version: input.page > 1 ? input.scopeVersion : undefined,
            sort_by: "period_end",
            sort_dir: "asc",
        },
    )

    let statements = pageRes.items ?? []
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
        input.ownerUserIds ? `对账负责人=${input.ownerUserIds}` : null,
        input.operatorUserIds ? `差异处理人=${input.operatorUserIds}` : null,
        input.handlerUserIds ? `当前复核人=${input.handlerUserIds}` : null,
        input.orgUnitIds ? `业务组织=${input.orgUnitIds}` : null,
        input.periodFrom || input.periodTo
            ? `期间=${input.periodFrom ?? "…"} ~ ${input.periodTo ?? "…"}`
            : null,
        input.q ? `搜索=${input.q}` : null,
    ].filter(Boolean)

    const noScope = pageRes.empty_reason === "no_scope"
    const emptyReason = noScope
        ? "NO_SCOPE"
        : total === 0
          ? hasAppliedSettlementFilters(input)
              ? "FILTER_NO_RESULT"
              : "NO_STATEMENTS"
          : undefined

    return {
        view: input.view,
        rows,
        page: pageRes.page ?? input.page,
        pageSize: pageRes.page_size ?? pageSize,
        total: noScope ? 0 : total,
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
        emptyReason,
        hasModulePermission: true,
        permissionVersion: "server",
        sourceAsOf: queriedAt,
        queriedAt,
        filterSummary: filterParts.length
            ? filterParts.join(" · ")
            : "默认待处理视图",
        scopeVersion: pageRes.scope_version,
        ownerOptions: pageRes.owner_options ?? [],
        operatorOptions: pageRes.operator_options ?? [],
        handlerOptions: pageRes.handler_options ?? [],
    }
}
