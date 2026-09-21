"use client"

import * as React from "react"

import {
    countSelectedByTab,
    filterMatrixByKeyword,
    matrixGroupsForTab,
    type PermissionPanelTab,
} from "@/features/admin/lib/permission-catalog"
import {
    filterPermissionView,
    orderPermissionGroups,
    type PermissionView,
} from "@/features/admin/lib/permission-editor"

export function usePermissionPanel(
    selected: readonly string[],
    initial: readonly string[] = [],
    view: PermissionView = "all",
) {
    const [keyword, setKeyword] = React.useState("")
    const [tab, setTab] = React.useState<PermissionPanelTab>(() => {
        const counts = countSelectedByTab(initial)
        return counts.business === 0 && counts.system > 0
            ? "system"
            : "business"
    })
    const [requestedGroup, setActiveGroup] = React.useState<string | null>(null)
    const selectedSet = React.useMemo(() => new Set(selected), [selected])
    const tabGroups = React.useMemo(
        () => orderPermissionGroups(matrixGroupsForTab(tab)),
        [tab],
    )
    const visibleGroups = React.useMemo(
        () =>
            filterPermissionView(
                filterMatrixByKeyword(tabGroups, keyword.trim().toLowerCase()),
                view,
                selected,
                initial,
            ),
        [tabGroups, keyword, view, selected, initial],
    )
    const activeGroup =
        visibleGroups.find((group) => group.name === requestedGroup) ??
        visibleGroups.find((group) =>
            group.codes.some((code) => initial.includes(code)),
        ) ??
        visibleGroups[0] ??
        null

    const progressByGroup = React.useMemo(
        () =>
            tabGroups.map((group) => ({
                name: group.name,
                selected: group.codes.filter((code) => selectedSet.has(code))
                    .length,
                total: group.codes.length,
            })),
        [tabGroups, selectedSet],
    )

    return {
        keyword,
        setKeyword,
        tab,
        setTab,
        activeGroup,
        setActiveGroup,
        visibleGroups,
        progressByGroup,
        selectedSet,
    }
}
