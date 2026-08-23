"use client"

import * as React from "react"

import {
    countSelectedByTab,
    filterMatrixByKeyword,
    matrixGroupsForTab,
    type PermissionMatrixGroup,
    type PermissionPanelTab,
} from "@/features/admin/lib/permission-catalog"

export type PermissionGroupProgress = {
    name: string
    selected: number
    total: number
}

/**
 * 权限面板状态：维度 Tab、关键词过滤、分组进度与当前定位组。
 * 纯状态层；矩阵渲染与滚动由 components/roles/permission-panel 负责。
 */
export function usePermissionPanel(selected: readonly string[]) {
    const [keyword, setKeyword] = React.useState("")
    const [tab, setTab] = React.useState<PermissionPanelTab>("business")
    const [activeGroup, setActiveGroup] = React.useState<string | null>(null)

    const q = keyword.trim().toLowerCase()
    const selectedSet = React.useMemo(() => new Set(selected), [selected])

    const tabGroups = React.useMemo(() => matrixGroupsForTab(tab), [tab])
    const visibleGroups = React.useMemo(
        () => filterMatrixByKeyword(tabGroups, q),
        [q, tabGroups],
    )

    // 搜索或切换维度后，把当前组定位到第一个仍可见的组
    React.useEffect(() => {
        if (!visibleGroups.some((group) => group.name === activeGroup)) {
            setActiveGroup(visibleGroups[0]?.name ?? null)
        }
    }, [visibleGroups, activeGroup])

    /** 分组进度按完整目录统计，不随搜索变化，避免「已选数」跟着筛选跳。 */
    const progressByGroup = React.useMemo<
        readonly PermissionGroupProgress[]
    >(
        () =>
            tabGroups.map((group) => ({
                name: group.name,
                selected: group.codes.filter((code) => selectedSet.has(code))
                    .length,
                total: group.codes.length,
            })),
        [selectedSet, tabGroups],
    )

    const selectedCountByTab = React.useMemo(
        () => countSelectedByTab(selected),
        [selected],
    )

    return {
        keyword,
        setKeyword,
        tab,
        setTab,
        activeGroup,
        setActiveGroup,
        /** 当前维度下经关键词过滤的矩阵组。 */
        visibleGroups,
        /** 当前维度的完整矩阵组（不受关键词影响）。 */
        tabGroups,
        progressByGroup,
        selectedCountByTab,
        selectedSet,
    }
}

export type { PermissionMatrixGroup }
