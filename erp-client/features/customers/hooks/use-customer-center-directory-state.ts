"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
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
import { useSearchDraft } from "@/lib/use-search-draft"

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
export type CustomerAppliedChip = Readonly<{
    key: "orgUnitIds"
    label: string
}>

/**
 * 客户中心目录的 URL 派生状态（docs/ui-filter-design.md §5）：
 * Applied 以 URL 为唯一事实源；Draft 只存本地，变化不触发请求。
 * 所有变更通过 router.replace(scroll: false) 写回 URL。
 * 更多筛选默认关闭，深链不自动展开。
 */
export function useCustomerCenterDirectoryState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const accountProfile = useAccountProfileQuery()
    const permissions = accountProfile.data?.permissions
    const canCreate = hasPermission(permissions, "customer:create")
    const canReadDirectory = hasPermission(permissions, "customer:list")
    const canOpenDetail = hasPermission(permissions, "customer:detail")
    const canReadAll = hasPermission(permissions, "customer_scope:detail")
    const canFilterOwner = hasPermission(permissions, "sales_person:list")
    const canFilterOrg = hasPermission(permissions, "org_unit:list")

    // ---- Applied：URL 唯一事实源；非法枚举值降级默认 ----
    const requestedScope = parseCustomerScope(searchParams.get("scope"))
    const scope = accountProfile.data && !canReadAll ? "mine" : requestedScope
    const hasUnauthorizedFilters =
        scope !== requestedScope ||
        (!canFilterOwner && searchParams.has("ownerUserIds")) ||
        (!canFilterOrg &&
            (searchParams.has("orgUnitIds") ||
                searchParams.has("includeDescendants")))
    const statusParam = searchParams.get("status")
    const status: DirectoryStatus =
        statusParam === "disabled" || statusParam === "all"
            ? statusParam
            : "active"
    const q = searchParams.get("q") ?? ""
    const ownerUserIds = canFilterOwner
        ? (searchParams.get("ownerUserIds") ?? "")
        : ""
    const orgUnitIds = canFilterOrg
        ? (searchParams.get("orgUnitIds") ?? "")
        : ""
    const includeDescendants =
        Boolean(orgUnitIds) && searchParams.get("includeDescendants") === "true"
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
    const page = hasUnauthorizedFilters
        ? 1
        : parsePage(searchParams.get("page"))

    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // ---- Draft：本地受控，提交前不请求 ----
    const { searchDraft, setSearchDraft } = useSearchDraft(q, {
        skipWhileFocused: true,
        inputRef: searchInputRef,
    })
    const [statusDraft, setStatusDraft] =
        React.useState<DirectoryStatus>(status)
    const [panelOpen, setPanelOpen] = React.useState(false)

    // URL 回填只同步 Draft
    React.useEffect(() => {
        setStatusDraft(status)
    }, [status])

    const pushState = React.useCallback(
        (next: CustomerCenterDirectoryPatch) => {
            router.replace(
                writeDirectoryUrl(pathname, {
                    scope: next.scope ?? scope,
                    ownerUserIds: canFilterOwner
                        ? (next.ownerUserIds ?? ownerUserIds)
                        : "",
                    orgUnitIds: canFilterOrg
                        ? (next.orgUnitIds ?? orgUnitIds)
                        : "",
                    includeDescendants:
                        canFilterOrg &&
                        (next.includeDescendants ?? includeDescendants),
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
            canFilterOwner,
            canFilterOrg,
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

    React.useEffect(() => {
        if (accountProfile.data && hasUnauthorizedFilters)
            pushState({ page: 1 })
    }, [accountProfile.data, hasUnauthorizedFilters, pushState])
    React.useEffect(() => {
        if (!canFilterOwner) setOwnerDraft("")
        if (!canFilterOrg) {
            setOrgDraft("")
            setDescendantsDraft(false)
            setPanelOpen(false)
        }
    }, [canFilterOwner, canFilterOrg])

    /** 查询与「应用筛选」提交全部草稿，回到第一页并关闭面板。 */
    const applyFilters = React.useCallback(() => {
        const orgUnitIdsNext = orgDraft.trim()
        pushState({
            q: searchDraft.trim(),
            status: statusDraft,
            ownerUserIds: ownerDraft,
            orgUnitIds: orgUnitIdsNext,
            includeDescendants: Boolean(orgUnitIdsNext) && descendantsDraft,
            page: 1,
        })
        setPanelOpen(false)
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

    const removeOrgFilter = React.useCallback(() => {
        setOrgDraft("")
        setDescendantsDraft(false)
        pushState({ orgUnitIds: "", includeDescendants: false, page: 1 })
    }, [pushState])

    /** 只清组织草稿；保留搜索、负责销售、状态和当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setOrgDraft("")
        setDescendantsDraft(false)
    }, [])

    /** 关闭面板时只恢复组织草稿，保留外部查询栏草稿。 */
    const cancelMoreFilters = React.useCallback(() => {
        setOrgDraft(orgUnitIds)
        setDescendantsDraft(includeDescendants)
        setPanelOpen(false)
    }, [includeDescendants, orgUnitIds])

    /** 同时重置 Draft、URL 筛选参数与分页；保留 scope/sort/dir。 */
    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setStatusDraft("active")
        setOwnerDraft("")
        setOrgDraft("")
        setDescendantsDraft(false)
        setPanelOpen(false)
        pushState({
            q: "",
            status: "active",
            ownerUserIds: "",
            orgUnitIds: "",
            includeDescendants: false,
            page: 1,
        })
    }, [pushState, setSearchDraft])

    const appliedChips = React.useMemo<readonly CustomerAppliedChip[]>(
        () =>
            orgUnitIds
                ? [
                      {
                          key: "orgUnitIds",
                          label: `组织：已选 ${orgUnitIds.split(",").length} 个${includeDescendants ? "（含下级）" : ""}`,
                      },
                  ]
                : [],
        [orgUnitIds, includeDescendants],
    )

    const hasStructuredFilters =
        status !== "active" || Boolean(ownerUserIds) || Boolean(orgUnitIds)
    const hasActiveFilters = hasStructuredFilters || q.trim().length > 0
    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        statusDraft !== status ||
        (canFilterOwner && ownerDraft !== ownerUserIds) ||
        (canFilterOrg &&
            (orgDraft !== orgUnitIds ||
                (Boolean(orgDraft) && descendantsDraft) !== includeDescendants))

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
            if (head && !SORT_COLUMN_TO_FIELD[head.id]) return
            pushState({
                dir: head?.desc === false ? "asc" : "desc",
                page: 1,
            })
        },
        [pushState],
    )

    return {
        accountProfile,
        canCreate,
        canReadDirectory,
        canOpenDetail,
        canReadAll,
        canFilterOwner,
        canFilterOrg,
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
        dir,
        page,
        searchDraft,
        setSearchDraft,
        statusDraft,
        setStatusDraft,
        panelOpen,
        setPanelOpen,
        searchInputRef,
        hasActiveFilters,
        hasPendingChanges,
        appliedChips,
        pushState,
        applyFilters,
        applyScope,
        removeOrgFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearAllFilters,
        handlePaginationChange,
        sorting,
        handleSortingChange,
    }
}
