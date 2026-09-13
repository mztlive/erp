"use client"

import { useMutation, useQuery } from "@tanstack/react-query"

import {
    fetchCostEntriesForRow,
    fetchPeriodBasisConfig,
    fetchProfitLossView,
    startProfitLossExport,
    type PeriodBasisConfigQuery,
} from "@/features/actual-profit-loss/api"
import type { ProfitLossQuery } from "@/features/actual-profit-loss/types"

const profitLossKeys = {
    all: ["actual-profit-loss"] as const,
    periodBasis: (q: PeriodBasisConfigQuery) =>
        [...profitLossKeys.all, "period-basis", q] as const,
    view: (query: ProfitLossQuery) =>
        [...profitLossKeys.all, "view", query] as const,
    costEntry: (id: string) =>
        [...profitLossKeys.all, "cost-entry", id] as const,
    costEntries: (ids: readonly string[], scopeVersion?: string) =>
        [
            ...profitLossKeys.all,
            "cost-entries",
            scopeVersion,
            [...ids].sort().join(","),
        ] as const,
}

export function usePeriodBasisConfigQuery(q: PeriodBasisConfigQuery = {}) {
    return useQuery({
        queryKey: profitLossKeys.periodBasis(q),
        queryFn: () => fetchPeriodBasisConfig(q),
        staleTime: 30_000,
    })
}

export function useProfitLossViewQuery(
    query: ProfitLossQuery | null,
    enabled: boolean,
) {
    return useQuery({
        queryKey: profitLossKeys.view(
            query ?? {
                from: "",
                to: "",
                periodBasis: "",
                scopeId: "",
                coverage: "covered",
                dimension: "sales_order",
                sort: "actualProfitLossNet:asc",
                page: 1,
                pageSize: 20,
            },
        ),
        queryFn: () => fetchProfitLossView(query!),
        enabled: enabled && query != null && Boolean(query.periodBasis),
    })
}

/** 成本读取缓存绑定本次范围版本，重新打开时重验独立详情接口。 */
export function useCostEntriesForRowQuery(
    costEntryIds: readonly string[],
    scopeVersion?: string,
) {
    return useQuery({
        queryKey: profitLossKeys.costEntries(costEntryIds, scopeVersion),
        queryFn: () => fetchCostEntriesForRow(costEntryIds),
        enabled: costEntryIds.length > 0,
        refetchOnMount: "always",
    })
}

export function useStartProfitLossExportMutation() {
    return useMutation({
        mutationFn: startProfitLossExport,
    })
}

export type { ProfitLossQuery }
