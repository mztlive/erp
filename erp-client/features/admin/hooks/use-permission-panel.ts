"use client"

import * as React from "react"

import {
    countSelectedByTab,
    filterMatrixByKeyword,
    isSystemGroup,
    matrixGroupsForTab,
    PERMISSION_BY_CODE,
    type PermissionPanelTab,
} from "@/features/admin/lib/permission-catalog"
import {
    filterPermissionView,
    orderPermissionGroups,
    type PermissionView,
} from "@/features/admin/lib/permission-editor"

export type FocusedPermissionGroup = {
    name: string
    seq: number
}

export function usePermissionPanel(
    selected: readonly string[],
    initial: readonly string[] = [],
    view: PermissionView = "all",
    focusedGroup?: FocusedPermissionGroup | null,
) {
    const [keyword, setKeyword] = React.useState("")
    const [tab, setTab] = React.useState<PermissionPanelTab>(() => {
        const counts = countSelectedByTab(initial)
        return counts.business === 0 && counts.system > 0
            ? "system"
            : "business"
    })
    const [requestedGroup, setActiveGroup] = React.useState<string | null>(null)

    const focusedName = focusedGroup?.name
    const focusedSeq = focusedGroup?.seq
    React.useEffect(() => {
        if (!focusedName || focusedSeq == null) return
        setTab(isSystemGroup(focusedName) ? "system" : "business")
        setKeyword("")
        setActiveGroup(focusedName)
    }, [focusedName, focusedSeq])

    const selectedSet = React.useMemo(() => new Set(selected), [selected])
    const initialSet = React.useMemo(() => new Set(initial), [initial])

    const tabGroups = React.useMemo(
        () => orderPermissionGroups(matrixGroupsForTab(tab)),
        [tab],
    )
    const otherTab: PermissionPanelTab =
        tab === "business" ? "system" : "business"
    const otherTabGroups = React.useMemo(
        () => orderPermissionGroups(matrixGroupsForTab(otherTab)),
        [otherTab],
    )

    const normalizedKeyword = keyword.trim().toLowerCase()

    const visibleGroups = React.useMemo(
        () =>
            filterPermissionView(
                filterMatrixByKeyword(tabGroups, normalizedKeyword),
                view,
                selected,
                initial,
            ),
        [tabGroups, normalizedKeyword, view, selected, initial],
    )

    const otherTabVisibleCount = React.useMemo(() => {
        const matched = filterPermissionView(
            filterMatrixByKeyword(otherTabGroups, normalizedKeyword),
            view,
            selected,
            initial,
        )
        return matched.reduce((sum, group) => sum + group.codes.length, 0)
    }, [otherTabGroups, normalizedKeyword, view, selected, initial])

    const activeGroup =
        visibleGroups.find((group) => group.name === requestedGroup) ??
        visibleGroups.find((group) =>
            group.codes.some((code) => initialSet.has(code)),
        ) ??
        visibleGroups[0] ??
        null

    const progressByGroup = React.useMemo(
        () =>
            tabGroups.map((group) => {
                let selectedCount = 0
                let hasChanges = false
                for (const code of group.codes) {
                    const isSelected = selectedSet.has(code)
                    if (isSelected) selectedCount += 1
                    if (isSelected !== initialSet.has(code)) hasChanges = true
                }
                return {
                    name: group.name,
                    selected: selectedCount,
                    total: group.codes.length,
                    hasChanges,
                }
            }),
        [tabGroups, selectedSet, initialSet],
    )

    const countsByTab = React.useMemo(
        () => countSelectedByTab(selected),
        [selected],
    )

    const viewCounts = React.useMemo(() => {
        let changed = 0
        let dangerous = 0
        for (const [code, item] of PERMISSION_BY_CODE) {
            const isSelected = selectedSet.has(code)
            if (isSelected !== initialSet.has(code)) changed += 1
            if (item.dangerous) dangerous += 1
        }
        return {
            all: PERMISSION_BY_CODE.size,
            selected: selectedSet.size,
            changed,
            dangerous,
        }
    }, [selectedSet, initialSet])

    return {
        keyword,
        setKeyword,
        tab,
        setTab,
        otherTab,
        otherTabVisibleCount,
        countsByTab,
        viewCounts,
        activeGroup,
        setActiveGroup,
        visibleGroups,
        progressByGroup,
        selectedSet,
    }
}
