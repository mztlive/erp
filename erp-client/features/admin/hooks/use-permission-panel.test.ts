import { act, renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import {
    BUSINESS_GROUPS,
    SYSTEM_GROUPS,
    filterGroupsByKeyword,
} from "@/features/admin/lib/permission-catalog"
import { usePermissionPanel } from "./use-permission-panel"

describe("usePermissionPanel", () => {
    it("starts on the business tab with the first business group active", () => {
        const { result } = renderHook(() => usePermissionPanel([]))

        expect(result.current.tab).toBe("business")
        expect(result.current.keyword).toBe("")
        expect(result.current.visibleGroups).toBe(BUSINESS_GROUPS)
        expect(result.current.activeGroup).toBe(BUSINESS_GROUPS[0]?.name)
        expect(result.current.currentGroup).toBe(BUSINESS_GROUPS[0])
        expect(result.current.selectedCountByTab).toEqual({
            business: 0,
            system: 0,
        })
    })

    it("filters groups by keyword and points at the first visible group", () => {
        expect(BUSINESS_GROUPS.length).toBeGreaterThan(1)
        const needle = BUSINESS_GROUPS[1].items[0].code
        const q = needle.toLowerCase()
        const expected = filterGroupsByKeyword(BUSINESS_GROUPS, q)

        const { result } = renderHook(() => usePermissionPanel([]))
        act(() => result.current.setKeyword(needle))

        expect(result.current.visibleGroups).toEqual(expected)
        expect(result.current.activeGroup).toBe(expected[0]?.name ?? null)
        expect(result.current.currentGroup).toBe(
            result.current.visibleGroups[0] ?? null,
        )
    })

    it("clears to an empty panel when nothing matches and restores on clear", () => {
        const { result } = renderHook(() => usePermissionPanel([]))

        act(() => result.current.setKeyword("__no_such_permission__"))
        expect(result.current.visibleGroups).toEqual([])
        expect(result.current.activeGroup).toBeNull()
        expect(result.current.currentGroup).toBeNull()

        act(() => result.current.setKeyword(""))
        expect(result.current.visibleGroups).toBe(BUSINESS_GROUPS)
        expect(result.current.activeGroup).toBe(BUSINESS_GROUPS[0]?.name)
        expect(result.current.currentGroup).toBe(BUSINESS_GROUPS[0])
    })

    it("treats whitespace-only keywords as empty (keeps original reference)", () => {
        const { result } = renderHook(() => usePermissionPanel([]))

        act(() => result.current.setKeyword("   "))
        expect(result.current.visibleGroups).toBe(BUSINESS_GROUPS)
    })

    it("switches dimension and relocates the active group", () => {
        expect(SYSTEM_GROUPS.length).toBeGreaterThan(0)
        const { result } = renderHook(() => usePermissionPanel([]))

        act(() => result.current.setTab("system"))
        expect(result.current.visibleGroups).toBe(SYSTEM_GROUPS)
        expect(result.current.activeGroup).toBe(SYSTEM_GROUPS[0]?.name)
        expect(result.current.currentGroup).toBe(SYSTEM_GROUPS[0])

        act(() => result.current.setTab("business"))
        expect(result.current.visibleGroups).toBe(BUSINESS_GROUPS)
        expect(result.current.activeGroup).toBe(BUSINESS_GROUPS[0]?.name)
    })

    it("keeps the manually selected group until it disappears from results", () => {
        expect(BUSINESS_GROUPS.length).toBeGreaterThan(1)
        const { result } = renderHook(() => usePermissionPanel([]))

        act(() => result.current.setActiveGroup(BUSINESS_GROUPS[1].name))
        expect(result.current.currentGroup).toBe(BUSINESS_GROUPS[1])

        const needle = BUSINESS_GROUPS[0].items[0].code
        const q = needle.toLowerCase()
        const expected = filterGroupsByKeyword(BUSINESS_GROUPS, q)
        act(() => result.current.setKeyword(needle))
        expect(result.current.visibleGroups).toEqual(expected)
        // 手动选中的组仍在结果里则保持，否则回落到第一个可见组
        if (expected.some((group) => group.name === BUSINESS_GROUPS[1].name)) {
            expect(result.current.activeGroup).toBe(BUSINESS_GROUPS[1].name)
        } else {
            expect(result.current.activeGroup).toBe(expected[0]?.name ?? null)
        }
    })

    it("recomputes per-tab selection counts and ignores unknown codes", () => {
        const businessCode = BUSINESS_GROUPS[0]?.items[0]?.code
        const systemCode = SYSTEM_GROUPS[0]?.items[0]?.code
        expect(businessCode).toBeDefined()
        expect(systemCode).toBeDefined()

        const { result, rerender } = renderHook(
            ({ selected }: { selected: string[] }) =>
                usePermissionPanel(selected),
            { initialProps: { selected: [] as string[] } },
        )
        expect(result.current.selectedCountByTab).toEqual({
            business: 0,
            system: 0,
        })

        rerender({ selected: [businessCode!, systemCode!, "ghost:view"] })
        expect(result.current.selectedCountByTab).toEqual({
            business: 1,
            system: 1,
        })

        rerender({ selected: [] })
        expect(result.current.selectedCountByTab).toEqual({
            business: 0,
            system: 0,
        })
    })
})
