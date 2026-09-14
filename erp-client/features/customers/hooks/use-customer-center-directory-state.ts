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
    ownerUserIds?: string
    orgUnitIds?: string
    includeDescendants?: boolean
    scope?: CustomerScope
    status?: DirectoryStatus
    q?: string
    sort?: string
    dir?: "asc" | "desc"
    page?: number
}

/** 可被单独移除的已生效筛选条件。 */
export type CustomerFilterKey = "q" | "status" | "ownerUserIds" | "orgUnitIds"

export type CustomerAppliedChip = Readonly<{
    key: CustomerFilterKey
    label: string
}>

/**
 * 客户中心目录的 URL 派生状态（docs/ui-filter-design.md §5）：
 * Applied 以 URL 为唯一事实源；Draft（关键词/状态）只存本地，
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
    const ownerUserIds = searchParams.get("ownerUserIds") ?? ""
    const orgUnitIds = searchParams.get("orgUnitIds") ?? ""
    const includeDescendants = searchParams.get("includeDescendants") === "true"
    const [ownerDraft, setOwnerDraft] = React.useState(ownerUserIds)
    const [orgDraft, setOrgDraft] = React.useState(orgUnitIds)
    const [descendantsDraft, setDescendantsDraft] =
        React.useState(includeDescendants)
    React.useEffect(() => setOwnerDraft(ownerUserIds), [ownerUserIds])
    React.useEffect(() => setOrgDraft(orgUnitIds), [orgUnitIds])
    React.useEffect(
        () => setDescendantsDraft(includeDescendants),
        [includeDescendants],
    )
    const sort = "business"
    const dir: "asc" | "desc" =
        searchParams.get("dir") === "asc" ? "asc" : "desc"
    const page = parsePage(searchParams.get("page"))

    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // ---- Draft：本地受控，提交前不请求 ----
    const [searchDraft, setSearchDraft] = React.useState(q)
    const [statusDraft, setStatusDraft] =
        React.useState<DirectoryStatus>(status)

    // URL 回填只同步 Draft
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(q)
        }
        setStatusDraft(status)
    }, [q, status, ownerUserIds])

    const pushState = React.useCallback(
        (next: CustomerCenterDirectoryPatch) => {
            router.replace(
                writeDirectoryUrl(pathname, {
                    scope: next.scope ?? scope,
                    ownerUserIds: next.ownerUserIds ?? ownerUserIds,
                    orgUnitIds: next.orgUnitIds ?? orgUnitIds,
                    includeDescendants:
                        next.includeDescendants ?? includeDescendants,
                    status: next.status ?? status,
                    q: next.q ?? q,
                    sort: next.sort ?? sort,
                    dir: next.dir ?? dir,
                    page: next.page ?? page,
                }),
                { scroll: false },
            )
        },
        [
            dir,
            includeDescendants,
            orgUnitIds,
            page,
            pathname,
            q,
            router,
            scope,
            sort,
            status,
            ownerUserIds,
        ],
    )

    /** 单一提交路径：查询按钮与搜索框 Enter 共用。 */
    const applyFilters = React.useCallback(() => {
        pushState({
            q: searchDraft.trim(),
            status: statusDraft,
            ownerUserIds: ownerDraft,
            orgUnitIds: orgDraft,
            includeDescendants: descendantsDraft,
            page: 1,
        })
    }, [
        pushState,
        searchDraft,
        statusDraft,
        ownerDraft,
        orgDraft,
        descendantsDraft,
    ])

    /** 快捷筛选（客户范围）直接写 Applied；不改动关键词或状态草稿。 */
    const applyScope = React.useCallback(
        (next: CustomerScope) => {
            pushState({ scope: next, page: 1 })
        },
        [pushState],
    )

    /** 移除单个已生效条件；状态移除后回到业务默认「启用」。 */
    const removeFilter = React.useCallback(
        (key: CustomerFilterKey) => {
            if (key === "ownerUserIds") {
                setOwnerDraft("")
                pushState({ ownerUserIds: "", page: 1 })
            }
            if (key === "orgUnitIds") {
                setOrgDraft("")
                setDescendantsDraft(false)
                pushState({
                    orgUnitIds: "",
                    includeDescendants: false,
                    page: 1,
                })
            }
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

    /** 仅重置更多条件草稿；本页无更多面板。 */
    const resetMoreFilters = React.useCallback(() => {
        return
    }, [])

    /** 同时重置 Draft、URL 筛选参数与分页；保留 scope/sort/dir。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setStatusDraft("active")
        setOwnerDraft("")
        setOrgDraft("")
        setDescendantsDraft(false)
        pushState({
            q: "",
            status: "active",
            ownerUserIds: "",
            orgUnitIds: "",
            includeDescendants: false,
            page: 1,
        })
    }, [pushState])

    /** 所有已生效筛选均可从 chip 单独撤销。 */
    const appliedChips = React.useMemo<readonly CustomerAppliedChip[]>(() => {
        const chips: CustomerAppliedChip[] = []
        if (ownerUserIds)
            chips.push({
                key: "ownerUserIds",
                label: `负责销售：已选 ${ownerUserIds.split(",").length} 人`,
            })
        if (orgUnitIds)
            chips.push({
                key: "orgUnitIds",
                label: `组织：已选 ${orgUnitIds.split(",").length} 个${includeDescendants ? "（含下级）" : ""}`,
            })
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
    }, [q, status, ownerUserIds, orgUnitIds, includeDescendants])

    const hasStructuredFilters =
        status !== "active" || Boolean(ownerUserIds) || Boolean(orgUnitIds)
    const hasActiveFilters = hasStructuredFilters || q.trim().length > 0
    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        statusDraft !== status ||
        ownerDraft !== ownerUserIds ||
        orgDraft !== orgUnitIds ||
        descendantsDraft !== includeDescendants

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
        ownerUserIds,
        ownerDraft,
        setOwnerDraft,
        orgUnitIds,
        orgDraft,
        setOrgDraft,
        includeDescendants,
        descendantsDraft,
        setDescendantsDraft,
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
        hasStructuredFilters,
        hasActiveFilters,
        hasPendingChanges,
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
export function useCustomerCenterScopeGuard() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const accountProfile = useAccountProfileQuery()
    const canCreate = hasPermission(
        accountProfile.data?.permissions,
        "customer:create",
    )
    const canReadAll = hasPermission(
        accountProfile.data?.permissions,
        "customer_scope:detail",
    )
    const scope = parseCustomerScope(searchParams.get("scope"))
    const statusParam = searchParams.get("status")
    const status: DirectoryStatus =
        statusParam === "disabled" || statusParam === "all"
            ? statusParam
            : "active"
    React.useEffect(() => {
        if (!accountProfile.data) return
        if (scope !== "all_authorized" || canReadAll) return
        router.replace(
            writeDirectoryUrl(pathname, {
                scope: "mine",
                ownerUserIds: searchParams.get("ownerUserIds") ?? "",
                orgUnitIds: searchParams.get("orgUnitIds") ?? "",
                includeDescendants:
                    searchParams.get("includeDescendants") === "true",
                status,
                q: searchParams.get("q") ?? "",
                sort: "business",
                dir: searchParams.get("dir") === "asc" ? "asc" : "desc",
                page: 1,
            }),
            { scroll: false },
        )
    }, [
        accountProfile.data,
        canReadAll,
        pathname,
        router,
        scope,
        searchParams,
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
