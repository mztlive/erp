"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    parsePage,
    SORT_COLUMN_TO_FIELD,
    writeDirectoryUrl,
} from "@/features/customers/lib/directory-url"
import type { DirectoryStatus } from "@/features/customers/lib/directory-url"
import { parseCustomerScope } from "@/features/customers/lib/filter-customers"
import type { CustomerScope } from "@/features/customers/types"
import { hasPermission } from "@/lib/permissions"

/** 目录筛选/分页/排序的 URL 参数增量。 */
export type CustomerCenterDirectoryPatch = {
    scope?: CustomerScope
    status?: DirectoryStatus
    q?: string
    sort?: string
    dir?: "asc" | "desc"
    page?: number
}

/**
 * 客户中心目录的 URL 派生状态：范围/状态/关键词/排序/分页全部从 URL 读取，
 * 所有变更通过 router.replace 写回 URL（P6：分页从 URL 派生，筛选变更回第 1 页）。
 */
export function useCustomerCenterDirectoryState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const scope = parseCustomerScope(searchParams.get("scope"))
    const statusParam = searchParams.get("status")
    const status: DirectoryStatus =
        statusParam === "disabled" || statusParam === "all"
            ? statusParam
            : "active"
    const q = searchParams.get("q") ?? ""
    const sort = "business"
    const dir: "asc" | "desc" =
        searchParams.get("dir") === "asc" ? "asc" : "desc"
    const page = parsePage(searchParams.get("page"))

    const [searchDraft, setSearchDraft] = React.useState(q)

    React.useEffect(() => {
        setSearchDraft(q)
    }, [q])

    const pushState = React.useCallback(
        (next: CustomerCenterDirectoryPatch) => {
            router.replace(
                writeDirectoryUrl(pathname, {
                    scope: next.scope ?? scope,
                    status: next.status ?? status,
                    q: next.q ?? q,
                    sort: next.sort ?? sort,
                    dir: next.dir ?? dir,
                    page: next.page ?? page,
                }),
            )
        },
        [dir, page, pathname, q, router, scope, sort, status],
    )

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            pushState({ page: next.pageIndex + 1 })
        },
        [pushState],
    )

    const sorting = React.useMemo<SortingState>(
        () => [{ id: sort, desc: dir === "desc" }],
        [dir, sort],
    )

    const handleSortingChange = React.useCallback(
        (next: SortingState) => {
            const head = next[0]
            if (!head || !SORT_COLUMN_TO_FIELD[head.id]) return
            pushState({
                sort: head.id,
                dir: head.desc ? "desc" : "asc",
                page: 1,
            })
        },
        [pushState],
    )

    /** P4：清 q/status/分页，保留 scope（视图）与 sort/dir（排序）。 */
    const clearFilters = () => {
        setSearchDraft("")
        router.replace(
            writeDirectoryUrl(pathname, {
                scope,
                status: "active",
                q: "",
                sort,
                dir,
                page: 1,
            }),
        )
    }

    const hasActiveFilters = status !== "active" || q.trim().length > 0

    return {
        scope,
        status,
        q,
        sort,
        dir,
        page,
        searchDraft,
        setSearchDraft,
        pushState,
        handlePaginationChange,
        sorting,
        handleSortingChange,
        clearFilters,
        hasActiveFilters,
    }
}

/**
 * 权限守卫：无「全部有权客户」范围权限时，访问 all_authorized 一律重定向到
 * 我的客户视图（等账号资料加载完成后再判定，避免闪跳）。
 */
export function useCustomerCenterScopeGuard(state: {
    scope: CustomerScope
    status: DirectoryStatus
    q: string
    sort: string
    dir: "asc" | "desc"
    page: number
}) {
    const router = useRouter()
    const pathname = usePathname()
    const accountProfile = useAccountProfileQuery()
    const canCreate = hasPermission(
        accountProfile.data?.permissions,
        "customer:create",
    )
    const canReadAll = hasPermission(
        accountProfile.data?.permissions,
        "customer_scope:detail",
    )
    const { scope, status, q, sort, dir, page } = state

    React.useEffect(() => {
        if (
            !accountProfile.isPending &&
            scope === "all_authorized" &&
            !canReadAll
        ) {
            router.replace(
                writeDirectoryUrl(pathname, {
                    scope: "mine",
                    status,
                    q,
                    sort,
                    dir,
                    page: 1,
                }),
            )
        }
    }, [
        accountProfile.isPending,
        canReadAll,
        dir,
        page,
        pathname,
        q,
        router,
        scope,
        sort,
        status,
    ])

    return { accountProfile, canCreate, canReadAll }
}

/** 在非输入控件焦点下按 “/” 聚焦客户搜索框。 */
export function useCustomerCenterSearchShortcut() {
    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            if (
                target &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)
            ) {
                return
            }
            if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        '[data-slot="customer-search"]',
                    )
                    ?.focus()
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [])
}
