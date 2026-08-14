"use client"

import * as React from "react"

import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import { useSlashSearchHotkey } from "@/features/master-data/hooks/use-slash-search-hotkey"
import {
    buildCategoryForest,
    flattenCategoryForest,
    type CategoryTreeNode,
} from "@/features/master-data/lib/category-tree-model"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    buildMasterDataExportCsv,
    downloadCsv,
} from "@/features/master-data/lib/export-csv"
import type { MasterDataListItem } from "@/features/master-data/types"

/** W14 商品分类树页状态：搜索、启停筛选、展开/选中与写操作弹窗。 */
export function useMasterDataCategoryTree(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const [search, setSearch] = React.useState("")
    const [lifecycleStatus, setLifecycleStatus] = React.useState<
        "enabled" | "disabled" | "all"
    >("all")
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
        q: search,
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
    const filterActive = search.trim() !== "" || lifecycleStatus !== "all"

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

    const clearFilters = () => {
        setSearch("")
        setLifecycleStatus("all")
    }

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
        search,
        setSearch,
        lifecycleStatus,
        setLifecycleStatus,
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
