"use client"

import * as React from "react"

import {
    BUSINESS_GROUPS,
    SYSTEM_GROUPS,
    countSelectedByTab,
    filterGroupsByKeyword,
    type PermissionGroupOption,
    type PermissionPanelTab,
} from "@/features/admin/lib/permission-catalog"

/**
 * 权限面板状态：维度 Tab、组定位、关键词过滤与已选统计。
 * 纯状态层；UI 由 components/roles/permission-panel 渲染。
 */
export function usePermissionPanel(selected: readonly string[]) {
    const [keyword, setKeyword] = React.useState("")
    const [tab, setTab] = React.useState<PermissionPanelTab>("business")
    const [activeGroup, setActiveGroup] = React.useState<string | null>(null)

    const q = keyword.trim().toLowerCase()
    const sourceGroups: readonly PermissionGroupOption[] =
        tab === "system" ? SYSTEM_GROUPS : BUSINESS_GROUPS

    const visibleGroups = React.useMemo(
        () => filterGroupsByKeyword(sourceGroups, q),
        [q, sourceGroups],
    )

    // 搜索或切换维度后，把当前组定位到第一个仍可见的组
    React.useEffect(() => {
        if (!visibleGroups.some((group) => group.name === activeGroup)) {
            setActiveGroup(visibleGroups[0]?.name ?? null)
        }
    }, [visibleGroups, activeGroup])

    const currentGroup =
        visibleGroups.find((group) => group.name === activeGroup) ?? null

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
        visibleGroups,
        currentGroup,
        selectedCountByTab,
    }
}
