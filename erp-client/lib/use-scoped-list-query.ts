"use client"

/**
 * 范围列表查询：第二页起自动绑定首个响应的范围版本，避免拼接不同授权页。
 *
 * 客户目录、合同、销售单、采购单、供应商订单与三张资金范围列表原本各自
 * 复制同一段首屏基线逻辑；这里只收敛分页与范围版本，查询键和取数函数仍由
 * 各 feature 提供。
 */

import {
    useQuery,
    useQueryClient,
    type QueryKey,
    type UseQueryOptions,
} from "@tanstack/react-query"

export type ScopeVersionedListQuery = {
    page?: number
    scopeVersion?: string
}

export function useScopedListQuery<
    TQuery extends ScopeVersionedListQuery,
    TResult extends { scopeVersion?: string },
>({
    query,
    queryKey,
    fetchPage,
    enabled = true,
    keepPreviousData = false,
}: {
    query: TQuery
    queryKey: (query: TQuery) => QueryKey
    fetchPage: (query: TQuery) => Promise<TResult>
    enabled?: boolean
    /** 切换筛选时保留上一批结果，避免整卡闪烁。 */
    keepPreviousData?: boolean
}) {
    const client = useQueryClient()
    const page = query.page ?? 1
    const firstPage = { ...query, page: 1, scopeVersion: undefined } as TQuery
    const baseline = client.getQueryData<TResult>(queryKey(firstPage))
    const scopeVersion =
        query.scopeVersion ?? (page > 1 ? baseline?.scopeVersion : undefined)
    const scoped = { ...query, scopeVersion } as TQuery

    const options: UseQueryOptions<TResult, Error, TResult, QueryKey> = {
        queryKey: queryKey(scoped),
        queryFn: async () => {
            if (page === 1 || scopeVersion) return fetchPage(scoped)
            // 直接打开后续页时先建立本次查询的第一页授权快照。
            const first = await client.fetchQuery({
                queryKey: queryKey(firstPage),
                queryFn: () => fetchPage(firstPage),
            })
            return fetchPage({
                ...query,
                scopeVersion: first.scopeVersion,
            } as TQuery)
        },
        enabled,
    }
    if (keepPreviousData) {
        options.placeholderData = (previous) => previous
    }
    return useQuery(options)
}
