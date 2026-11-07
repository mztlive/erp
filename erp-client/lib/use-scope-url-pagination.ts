"use client"

/**
 * 范围查询列表（客户往来 / 供应商往来 / 开票申请）共用的 URL 分页与参数修补。
 *
 * 三处页面的页码解析、分页状态和 `patchUrl` 语义原本逐字重复；筛选字段仍由
 * 各 hook 自己解析与写回，这里只收敛与业务字段无关的公共部分。
 */

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"

export function useScopeUrlPagination({
    view,
    pageSize,
    scopeVersion,
}: {
    /** 提供时，patch 结果缺少 view 参数则回填当前视图。 */
    view?: string
    pageSize: number
    scopeVersion?: string
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const pageFromUrl = React.useMemo(() => {
        const page = Number.parseInt(searchParams.get("page") ?? "1", 10)
        return Number.isFinite(page) && page >= 1 ? page : 1
    }, [searchParams])

    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: Math.max(0, pageFromUrl - 1), pageSize }),
        [pageFromUrl, pageSize],
    )

    const patchUrl = React.useCallback(
        (
            patch: Record<string, string | null | undefined>,
            options?: { replace?: boolean },
        ) => {
            patchSearchParams(
                { router, pathname, searchParams, view },
                patch,
                options?.replace ? { replace: true, scroll: false } : options,
            )
        },
        [pathname, router, searchParams, view],
    )

    const handlePaginationChange = React.useCallback(
        (next: PaginationState, nextScopeVersion?: string) => {
            patchUrl(
                {
                    page:
                        next.pageIndex + 1 > 1
                            ? String(next.pageIndex + 1)
                            : null,
                    scopeVersion:
                        next.pageIndex + 1 > 1
                            ? (nextScopeVersion ?? scopeVersion)
                            : null,
                },
                { replace: true },
            )
        },
        [patchUrl, scopeVersion],
    )

    return {
        searchParams,
        pageFromUrl,
        pagination,
        patchUrl,
        handlePaginationChange,
    }
}
