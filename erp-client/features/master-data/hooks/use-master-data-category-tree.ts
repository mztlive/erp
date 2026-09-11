"use client"

import * as React from "react"
import {
    useListUrl,
    useSearchDraft,
} from "@/features/master-data/hooks/use-list-url"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import { useCreatePermission } from "@/features/master-data/hooks/use-create-permission"
import { useSlashSearchHotkey } from "@/features/master-data/hooks/use-slash-search-hotkey"
import {
    buildCategoryForest,
    filterCategoryForest,
    flattenCategoryForest,
    type CategoryTreeNode,
} from "@/features/master-data/lib/category-tree-model"
import { parseLifecycleStatus } from "@/features/master-data/lib/list-filters"
import {
    buildMasterDataExportCsv,
    downloadCsv,
} from "@/features/master-data/lib/export-csv"
import type { MasterDataListItem } from "@/features/master-data/types"

const EXPANDED_STORAGE_KEY = "category-tree-expanded"

function readStoredExpanded(): Set<string> | null {
    try {
        const saved: unknown = JSON.parse(
            sessionStorage.getItem(EXPANDED_STORAGE_KEY) ?? "null",
        )
        if (Array.isArray(saved) && saved.every((id) => typeof id === "string"))
            return new Set(saved)
    } catch {
        /* 本地偏好不可用时采用默认展开。 */
    }
    return null
}

function writeStoredExpanded(ids: ReadonlySet<string>) {
    try {
        sessionStorage.setItem(EXPANDED_STORAGE_KEY, JSON.stringify([...ids]))
    } catch {
        /* 不影响分类维护。 */
    }
}

function toggleExpandedId(ids: ReadonlySet<string>, id: string): Set<string> {
    return ids.has(id)
        ? new Set([...ids].filter((item) => item !== id))
        : new Set([...ids, id])
}

function ancestorIds(
    flat: readonly CategoryTreeNode[],
    startId: string | null | undefined,
    includeSelf: boolean,
): string[] {
    const ids: string[] = []
    let node = flat.find((entry) => entry.item.stableId === startId)
    if (!node) return ids
    if (includeSelf) ids.push(node.item.stableId)
    while (node.item.parentStableId) {
        ids.push(node.item.parentStableId)
        node = flat.find(
            (entry) => entry.item.stableId === node?.item.parentStableId,
        )
        if (!node) break
    }
    return ids
}

/** 分类导航：后端负责匹配，完整树负责祖先路径；选中与筛选互相独立。 */
export function useMasterDataCategoryTree(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const { searchParams, patchUrl, q } = useListUrl()
    const lifecycleStatus = parseLifecycleStatus(
        searchParams.get("lifecycleStatus"),
    )
    const { searchDraft, setSearchDraft } = useSearchDraft(q, searchInputRef)
    const selectedId = searchParams.get("category")
    const [expandedState, setExpanded] =
        React.useState<ReadonlySet<string> | null>(null)
    const [filterExpansion, setFilterExpansion] = React.useState<{
        key: string
        ids: ReadonlySet<string>
    } | null>(null)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [createParentId, setCreateParentId] = React.useState<
        string | undefined
    >()
    const [reviseTarget, setReviseTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [moveTarget, setMoveTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [disableTarget, setDisableTarget] =
        React.useState<MasterDataListItem | null>(null)
    const { canCreate, createBlockedReason } = useCreatePermission(
        "product_category:create",
    )
    useSlashSearchHotkey(searchInputRef)

    const fullQuery = useMasterDataListQuery({
        resource: "categories",
        lifecycleStatus: "all",
        revisionTiming: "all",
    })
    const listQuery = useMasterDataListQuery({
        resource: "categories",
        q: q.trim() || undefined,
        lifecycleStatus,
        revisionTiming: "all",
    })
    const rows = React.useMemo(
        () => fullQuery.data?.rows ?? [],
        [fullQuery.data],
    )
    const matchedRows = React.useMemo(
        () => (listQuery.isPlaceholderData ? [] : (listQuery.data?.rows ?? [])),
        [listQuery.data, listQuery.isPlaceholderData],
    )
    const fullForest = React.useMemo(() => buildCategoryForest(rows), [rows])
    const flat = React.useMemo(
        () => flattenCategoryForest(fullForest),
        [fullForest],
    )
    const matchedIds = React.useMemo(
        () => new Set(matchedRows.map((row) => row.stableId)),
        [matchedRows],
    )
    const filterActive = Boolean(q.trim()) || lifecycleStatus !== "all"
    const forest = React.useMemo(
        () =>
            filterActive
                ? filterCategoryForest(fullForest, matchedIds)
                : fullForest,
        [filterActive, fullForest, matchedIds],
    )
    const filterKey = `${q}:${lifecycleStatus}`
    const unfilteredDefaultExpanded = React.useMemo(
        () => new Set(fullForest.map((node) => node.item.stableId)),
        [fullForest],
    )
    const filteredDefaultExpanded = React.useMemo(
        () =>
            new Set(
                flattenCategoryForest(forest).map((node) => node.item.stableId),
            ),
        [forest],
    )
    const expanded = filterActive
        ? filterExpansion?.key === filterKey
            ? filterExpansion.ids
            : filteredDefaultExpanded
        : (expandedState ?? unfilteredDefaultExpanded)

    React.useEffect(() => {
        const saved = readStoredExpanded()
        if (saved) setExpanded(saved)
    }, [])
    const changeExpanded = (ids: ReadonlySet<string>) => {
        if (filterActive) setFilterExpansion({ key: filterKey, ids })
        else {
            setExpanded(ids)
            writeStoredExpanded(ids)
        }
    }
    const toggle = (id: string) => {
        changeExpanded(toggleExpandedId(expanded, id))
    }
    const setSelectedId = (id: string | null) => {
        const parents = ancestorIds(flat, id, false)
        if (parents.some((parentId) => !expanded.has(parentId)))
            changeExpanded(new Set([...expanded, ...parents]))
        patchUrl({ category: id })
    }
    const selectedNode = flat.find((node) => node.item.stableId === selectedId)
    const selected = selectedNode?.item ?? null
    const applyTreeFilters = () => {
        setFilterExpansion(null)
        patchUrl({ q: searchDraft.trim() || null })
    }
    const clearFilters = () => {
        setSearchDraft("")
        setFilterExpansion(null)
        patchUrl({ q: null, lifecycleStatus: null })
    }
    const onCreated = (id: string) => {
        if (createParentId) {
            const next = new Set([
                ...(expandedState ?? unfilteredDefaultExpanded),
                ...ancestorIds(flat, createParentId, true),
            ])
            setExpanded(next)
            writeStoredExpanded(next)
        }
        setSearchDraft("")
        patchUrl({ category: id, q: null, lifecycleStatus: null })
    }
    return {
        searchDraft,
        setSearchDraft,
        q,
        applyTreeFilters,
        lifecycleStatus,
        setLifecycleStatus: (value: "enabled" | "disabled" | "all") => {
            setFilterExpansion(null)
            patchUrl({ lifecycleStatus: value === "all" ? null : value })
        },
        clearSearch: () => {
            setSearchDraft("")
            patchUrl({ q: null })
        },
        hasPendingChanges: searchDraft.trim() !== q.trim(),
        selectedId,
        setSelectedId,
        selected,
        selectedNode,
        expanded,
        createOpen,
        setCreateOpen,
        createParentId,
        onCreated,
        reviseTarget,
        setReviseTarget,
        moveTarget,
        setMoveTarget,
        disableTarget,
        setDisableTarget,
        listQuery,
        fullQuery,
        rows,
        matchedRows,
        matchedIds,
        forest,
        fullForest,
        flat,
        filterActive,
        canCreate,
        createBlockedReason,
        toggle,
        expandAll: () =>
            changeExpanded(
                new Set(
                    flattenCategoryForest(forest).map(
                        (node) => node.item.stableId,
                    ),
                ),
            ),
        collapseAll: () => changeExpanded(new Set()),
        clearFilters,
        openCreateRoot: () => {
            setCreateParentId(undefined)
            setCreateOpen(true)
        },
        openCreateChild: (item: MasterDataListItem) => {
            setCreateParentId(item.stableId)
            setCreateOpen(true)
        },
        onExport: () =>
            downloadCsv(
                buildMasterDataExportCsv(matchedRows, "分类=商品分类"),
                "基础资料-商品分类",
            ),
        returnTo: `/master-data/categories${searchParams.size ? `?${searchParams}` : ""}`,
    }
}
