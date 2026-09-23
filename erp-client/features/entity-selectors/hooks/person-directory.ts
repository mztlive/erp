"use client"

import { useQuery } from "@tanstack/react-query"

import {
    fetchPersonDirectory,
    fetchSelectedPeople,
    type PersonDirectoryCategory,
    type PersonDirectoryPage,
} from "@/features/entity-selectors/api/person-directory"
import { queryKeyRoots } from "@/lib/query-key-roots"

export type PersonDirectoryFilters = {
    category: PersonDirectoryCategory
    q: string
    orgUnitIds: readonly string[]
    includeDescendants: boolean
    pageCount: number
}

export function personDirectoryListKey(filters: PersonDirectoryFilters) {
    return [
        ...queryKeyRoots.entitySelectors,
        "person-directory",
        filters.category,
        "list",
        {
            q: filters.q,
            orgUnitIds: [...filters.orgUnitIds].sort(),
            includeDescendants:
                filters.orgUnitIds.length > 0 && filters.includeDescendants,
            pageCount: filters.pageCount,
        },
    ] as const
}

export function personDirectorySelectedKey(
    category: PersonDirectoryCategory,
    ids: readonly string[],
) {
    return [
        ...queryKeyRoots.entitySelectors,
        "person-directory",
        category,
        "selected",
        [...ids].sort(),
    ] as const
}

/** 同一查询条件下连续读取已请求的页，后续页携带第一页的目录版本。 */
export function usePersonDirectoryList(filters: PersonDirectoryFilters) {
    return useQuery({
        queryKey: personDirectoryListKey(filters),
        queryFn: async (): Promise<{
            pages: PersonDirectoryPage[]
            scopeVersion: string
            policyVersion: number
            organizationVersion: number
        }> => {
            const first = await fetchPersonDirectory({
                category: filters.category,
                q: filters.q,
                page: 1,
                orgUnitIds: filters.orgUnitIds,
                includeDescendants: filters.includeDescendants,
            })
            const pages = [first]
            const lastPage = Math.min(
                filters.pageCount,
                Math.max(1, Math.ceil(first.total / first.page_size) || 1),
            )
            for (let page = 2; page <= lastPage; page += 1) {
                pages.push(
                    await fetchPersonDirectory({
                        category: filters.category,
                        q: filters.q,
                        page,
                        orgUnitIds: filters.orgUnitIds,
                        includeDescendants: filters.includeDescendants,
                        scopeVersion: first.scope_version,
                    }),
                )
            }
            return {
                pages,
                scopeVersion: first.scope_version,
                policyVersion: first.policy_version,
                organizationVersion: first.organization_version,
            }
        },
        staleTime: 0,
        placeholderData: undefined,
    })
}

/** 已选项回显。失败保持错误，不把结果收成空列表。 */
export function usePersonDirectorySelected(
    category: PersonDirectoryCategory,
    ids: readonly string[],
) {
    const selectedIds = [...ids].sort()
    return useQuery({
        queryKey: personDirectorySelectedKey(category, selectedIds),
        queryFn: () => fetchSelectedPeople(category, selectedIds),
        enabled: selectedIds.length > 0,
        staleTime: 0,
        placeholderData: undefined,
    })
}
