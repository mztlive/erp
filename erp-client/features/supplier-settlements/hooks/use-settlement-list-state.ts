"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"

/** 列表分页与筛选状态：以 URL 为唯一事实源（page 不在本地保留副本）。 */
export function useSettlementListState(
    urlState: SettlementsUrlState,
    patchUrl: (patch: Partial<SettlementsUrlState>) => void,
) {
    // pageSize 固定 50 不入 URL；财务列表不强制加排序（服务端无排序参数），记录在案。
    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, urlState.page - 1),
            pageSize: 50,
        }),
        [urlState.page],
    )

    // 清除=清全部筛选参数、view 回 pending（保持原清除语义）、分页回第 1 页；
    // 保留 preview/statementId/returnTo 等导航上下文。空态与工具栏常驻清除共用。
    const hasActiveFilters = Boolean(
        urlState.supplierId ||
            urlState.periodFrom ||
            urlState.periodTo ||
            urlState.status ||
            urlState.differenceType ||
            urlState.q ||
            urlState.view !== "pending",
    )
    const clearFilters = React.useCallback(() => {
        patchUrl({
            view: "pending",
            supplierId: undefined,
            status: undefined,
            differenceType: undefined,
            q: undefined,
            periodFrom: undefined,
            periodTo: undefined,
            page: 1,
        })
    }, [patchUrl])

    return { pagination, hasActiveFilters, clearFilters }
}
