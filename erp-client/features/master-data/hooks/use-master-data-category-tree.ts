"use client"

import * as React from "react"

import {
    useListUrl,
    useSearchDraft,
} from "@/features/master-data/hooks/use-list-url"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import { useSlashSearchHotkey } from "@/features/master-data/hooks/use-slash-search-hotkey"
import {
    buildCategoryForest,
    flattenCategoryForest,
    type CategoryTreeNode,
} from "@/features/master-data/lib/category-tree-model"
import { lifecycleFilterLabel } from "@/features/master-data/lib/copy"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { parseLifecycleStatus } from "@/features/master-data/lib/list-filters"
import {
    buildMasterDataExportCsv,
    downloadCsv,
} from "@/features/master-data/lib/export-csv"
import type { MasterDataListItem } from "@/features/master-data/types"

/** 分类树可被单独移除的已生效条件。 */
export type CategoryTreeFilterKey = "q" | "lifecycleStatus"

export type CategoryTreeAppliedChip = Readonly<{
    key: CategoryTreeFilterKey
    label: string
}>

/** W14 商品分类树页状态：URL 搜索/启停筛选、展开/选中与写操作弹窗。 */
export function useMasterDataCategoryTree(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const { searchParams, patchUrl, q } = useListUrl()
    const lifecycleStatus = parseLifecycleStatus(
        searchParams.get("lifecycleStatus"),
    )
    const { searchDraft, setSearchDraft } = useSearchDraft(q, searchInputRef)
    const [selectedId, setSelectedId] = React.useState<string | null>(null)
    const [expanded, setExpanded] = React.useState<Set<string>>(() => new Set())
    const [createOpen, setCreateOpen] = React.useState(false)
    const [createParentId, setCreateParentId] = React.useState<
        string | undefined
    >()
    const [reviseTarget, setReviseTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [disableTarget, setDisableTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [exportMeta, setExportMeta] = React.useState<{
        jobId: string
        rowCount: number
    } | null>(null)

    useSlashSearchHotkey(searchInputRef)

    const listQuery = useMasterDataListQuery({
        resource: "categories",
        q: q.trim() || undefined,
        lifecycleStatus,
        revisionTiming: "all",
    })

    const rows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data?.rows],
    )
    const forest = React.useMemo(() => buildCategoryForest(rows), [rows])
    const flat = React.useMemo(() => flattenCategoryForest(forest), [forest])

    // 首次加载默认展开全部根
    React.useEffect(() => {
        if (expanded.size > 0 || forest.length === 0) return
        setExpanded(new Set(forest.map((n) => n.item.stableId)))
    }, [forest, expanded.size])

    const selected =
        rows.find((r) => r.stableId === selectedId) ??
        flat.find((n) => n.item.stableId === selectedId)?.item ??
        null

    const selectedPath =
        flat.find((n) => n.item.stableId === selectedId)?.pathLabel ??
        selected?.name

    /** 可见节点数：根节点 + 展开父级下的所有后代（与界面实际渲染一致）。 */
    const visibleCount = React.useMemo(() => {
        return forest.reduce((count, node) => {
            let total = 1
            const walk = (n: CategoryTreeNode): void => {
                if (!expanded.has(n.item.stableId)) return
                for (const child of n.children) {
                    total += 1
                    walk(child)
                }
            }
            walk(node)
            return count + total
        }, 0)
    }, [forest, expanded])

    /** 搜索/启停筛选是否生效：空态与「系统从未建分类」区分。 */
    const filterActive = q.trim() !== "" || lifecycleStatus !== "all"

    /** 所有已生效条件均可从 chip 单独撤销。 */
    const appliedChips = React.useMemo<readonly CategoryTreeAppliedChip[]>(
        () => {
            const chips: CategoryTreeAppliedChip[] = []
            if (q.trim()) {
                chips.push({ key: "q", label: `搜索：${q.trim()}` })
            }
            if (lifecycleStatus !== "all") {
                chips.push({
                    key: "lifecycleStatus",
                    label: `启停：${lifecycleFilterLabel(lifecycleStatus)}`,
                })
            }
            return chips
        },
        [lifecycleStatus, q],
    )

    const toggle = React.useCallback((id: string) => {
        setExpanded((prev) => {
            const next = new Set(prev)
            if (next.has(id)) next.delete(id)
            else next.add(id)
            return next
        })
    }, [])

    const expandAll = () => {
        setExpanded(new Set(flat.map((n) => n.item.stableId)))
    }

    const collapseAll = () => {
        setExpanded(new Set())
    }

    /** 表单内 Enter：把搜索草稿写入 URL。 */
    const applyTreeFilters = React.useCallback(() => {
        const next = searchDraft.trim()
        if (next === q.trim()) return
        patchUrl({ q: next || null })
    }, [patchUrl, q, searchDraft])

    /** 启停是快捷筛选：直接写入 Applied URL。 */
    const setLifecycleStatus = React.useCallback(
        (next: "enabled" | "disabled" | "all") => {
            if (next === lifecycleStatus) return
            patchUrl({ lifecycleStatus: next === "all" ? null : next })
        },
        [lifecycleStatus, patchUrl],
    )

    /** 移除单个已生效条件。 */
    const removeFilter = React.useCallback(
        (key: CategoryTreeFilterKey) => {
            if (key === "q") setSearchDraft("")
            patchUrl({ [key]: null })
        },
        [patchUrl, setSearchDraft],
    )

    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        patchUrl({ q: null, lifecycleStatus: null })
    }, [patchUrl, setSearchDraft])

    const openCreateRoot = () => {
        setCreateParentId(undefined)
        setCreateOpen(true)
    }

    const openCreateChild = (parent: MasterDataListItem) => {
        setCreateParentId(parent.stableId)
        setCreateOpen(true)
    }

    const onExport = () => {
        if (rows.length === 0) return
        const csv = buildMasterDataExportCsv(
            rows,
            `分类=${masterDataCopy.categoryTreeTitle}`,
        )
        downloadCsv(csv, `基础资料-商品分类`)
        const datePart = new Date()
            .toISOString()
            .slice(0, 10)
            .replace(/-/g, "")
        setExportMeta({
            jobId: `导出-${datePart}-${String(Date.now() % 100000).padStart(5, "0")}`,
            rowCount: rows.length,
        })
    }

    return {
        searchDraft,
        setSearchDraft,
        lifecycleStatus,
        setLifecycleStatus,
        appliedChips,
        removeFilter,
        applyTreeFilters,
        selectedId,
        setSelectedId,
        expanded,
        createOpen,
        setCreateOpen,
        createParentId,
        reviseTarget,
        setReviseTarget,
        disableTarget,
        setDisableTarget,
        exportMeta,
        listQuery,
        rows,
        forest,
        flat,
        selected,
        selectedPath,
        visibleCount,
        filterActive,
        toggle,
        expandAll,
        collapseAll,
        clearFilters,
        openCreateRoot,
        openCreateChild,
        onExport,
    }
}
