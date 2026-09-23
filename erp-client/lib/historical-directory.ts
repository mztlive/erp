"use client"
import { useQuery } from "@tanstack/react-query"
import { apiGet } from "@/lib/api"

export type HistoricalDirectory = {
    attributionUserOptions: readonly { value: string; label: string }[]
    attributionOrgOptions: readonly { value: string; label: string }[]
    scopeVersion: string
    emptyReason?: string | null
}

/** 候选 key 仅绑定期间口径和客户，报表筛选及页码不得进入目录请求。 */
export function useHistoricalDirectory(
    path:
        | "/admin/customer-quality/history/directory"
        | "/admin/actual-profit-loss/history-directory",
    input: {
        from: string
        to: string
        customerId?: string
        periodBasis?: string
    } | null,
    enabled = true,
) {
    const context = {
        from: input?.from,
        to: input?.to,
        customer_id: input?.customerId,
        period_basis: input?.periodBasis,
    }
    return useQuery({
        queryKey: ["historical-directory", path, context],
        queryFn: () => apiGet<HistoricalDirectory>(path, context),
        enabled: enabled && Boolean(input?.from && input?.to),
        staleTime: 0,
    })
}
