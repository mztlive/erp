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

/** 可被单独移除的已生效筛选条件。 */
export type CustomerFilterKey = "q" | "status"

export type CustomerAppliedChip = Readonly<{
    key: CustomerFilterKey
    label: string
}>

/**
 * 客户中心目录的 URL 派生状态（docs/ui-filter-design.md §5）：
 * Applied 以 URL 为唯一事实源；Draft（关键词/状态）与 UI 态（面板展开）只存本地，
 * Draft 变化不触发请求。所有变更通过 router.replace(scroll: false) 写回 URL。
 */
export function useCustomerCenterDirectoryState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    // ---- Applied：URL 唯一事实源；非法枚举值降级默认 ----
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

    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // ---- Draft：本地受控，提交前不请求 ----
    const [searchDraft, setSearchDraft] = React.useState(q)
    const [statusDraft, setStatusDraft] =
        React.useState<DirectoryStatus>(status)

    // ---- UI 态 ----
    // 有结构化条件（状态非默认「启用」）的初始深链展开面板
    const [panelOpen, setPanelOpen] = React.useState(status !== "active")

    // URL 回填只同步 Draft；不抢夺用户当前面板展开态
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(q)
        }
        setStatusDraft(status)
    }, [q, status])

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
                { scroll: false },
            )
        },
        [dir, page, pathname, q, router, scope, sort, status],
    )

    /** 单一提交路径：收起态 Enter / 搜索框尾部箭头 / 展开态「应用全部筛选」都走这里。 */
    const applyFilters = React.useCallback(() => {
        pushState({ q: searchDraft.trim(), status: statusDraft, page: 1 })
        setPanelOpen(false)
    }, [pushState, searchDraft, statusDraft])

    /** 快捷筛选（客户范围）直接写 Applied；不改动关键词或「更多筛选」草稿。 */
    const applyScope = React.useCallback(
        (next: CustomerScope) => {
            pushState({ scope: next, page: 1 })
        },
        [pushState],
    )

    /** 移除单个已生效条件；状态移除后回到业务默认「启用」。 */
    const removeFilter = React.useCallback(
        (key: CustomerFilterKey) => {
            if (key === "q") {
                setSearchDraft("")
                pushState({ q: "", page: 1 })
            }
            if (key === "status") {
                setStatusDraft("active")
                pushState({ status: "active", page: 1 })
            }
        },
        [pushState],
    )

    /** 仅清除「更多筛选」；保留关键词和范围快捷筛选，面板保持展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setStatusDraft("active")
        pushState({ status: "active", page: 1 })
    }, [pushState])

    /** 同时重置 Draft、错误、面板、URL 筛选参数与分页；保留 scope/sort/dir。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setStatusDraft("active")
        setPanelOpen(false)
        pushState({ q: "", status: "active", page: 1 })
    }, [pushState])

    /** 所有已生效筛选均可从 chip 单独撤销。 */
    const appliedChips = React.useMemo<readonly CustomerAppliedChip[]>(() => {
        const chips: CustomerAppliedChip[] = []
        const trimmedQ = q.trim()
        if (trimmedQ) {
            chips.push({ key: "q", label: `搜索：${trimmedQ}` })
        }
        if (status !== "active") {
            chips.push({
                key: "status",
                label: status === "all" ? "状态：全部" : "状态：停用",
            })
        }
        return chips
    }, [q, status])

    const hasStructuredFilters = status !== "active"
    const hasActiveFilters = hasStructuredFilters || q.trim().length > 0

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

    return {
        scope,
        status,
        q,
        sort,
        dir,
        page,
        searchDraft,
        setSearchDraft,
        statusDraft,
        setStatusDraft,
        searchInputRef,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters,
        hasActiveFilters,
        appliedChips,
        pushState,
        applyFilters,
        applyScope,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
        handlePaginationChange,
        sorting,
        handleSortingChange,
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
                { scroll: false },
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

/** 在非输入控件焦点、且无 Dialog/Sheet 打开时按 “/” 聚焦客户搜索框。 */
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
            if (
                document.querySelector('[role="dialog"], [data-slot="sheet"]')
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
