import { act, renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import {
    filterMatrixByKeyword,
    matrixGroupsForTab,
} from "@/features/admin/lib/permission-catalog"
import { usePermissionPanel } from "./use-permission-panel"

const BUSINESS = matrixGroupsForTab("business")
const SYSTEM = matrixGroupsForTab("system")

describe("usePermissionPanel", () => {
    it("starts on the business tab with the first business group active", () => {
        const { result } = renderHook(() => usePermissionPanel([]))

        expect(result.current.tab).toBe("business")
        expect(result.current.keyword).toBe("")
        expect(result.current.visibleGroups).toEqual(BUSINESS)
        expect(result.current.activeGroup).toBe(BUSINESS[0]?.name)
        expect(result.current.selectedCountByTab).toEqual({
            business: 0,
            system: 0,
        })
    })

    it("filters groups by keyword and points at the first visible group", () => {
        expect(BUSINESS.length).toBeGreaterThan(1)
        const needle = BUSINESS[1]!.rows[0]!.codes[0]!
        const expected = filterMatrixByKeyword(BUSINESS, needle.toLowerCase())

        const { result } = renderHook(() => usePermissionPanel([]))
        act(() => result.current.setKeyword(needle))

        expect(result.current.visibleGroups).toEqual(expected)
        expect(result.current.activeGroup).toBe(expected[0]?.name ?? null)
    })

    it("keeps only matching rows and columns inside a matched group", () => {
        const group = BUSINESS.find((candidate) => candidate.actions.length > 1)
        expect(group).toBeDefined()
        const code = group!.rows[0]!.codes[0]!

        const { result } = renderHook(() => usePermissionPanel([]))
        act(() => result.current.setKeyword(code))

        const hit = result.current.visibleGroups.find(
            (candidate) => candidate.name === group!.name,
        )
        expect(hit).toBeDefined()
        expect(hit!.codes).toContain(code)
        // 命中项以外的列被裁掉，矩阵不再展示无关动作
        expect(hit!.codes.every((candidate) => candidate === code)).toBe(true)
    })

    it("clears to an empty panel when nothing matches and restores on clear", () => {
        const { result } = renderHook(() => usePermissionPanel([]))

        act(() => result.current.setKeyword("__no_such_permission__"))
        expect(result.current.visibleGroups).toEqual([])
        expect(result.current.activeGroup).toBeNull()

        act(() => result.current.setKeyword(""))
        expect(result.current.visibleGroups).toEqual(BUSINESS)
        expect(result.current.activeGroup).toBe(BUSINESS[0]?.name)
    })

    it("treats whitespace-only keywords as empty (keeps original reference)", () => {
        const { result } = renderHook(() => usePermissionPanel([]))

        act(() => result.current.setKeyword("   "))
        expect(result.current.visibleGroups).toEqual(BUSINESS)
    })

    it("switches dimension and relocates the active group", () => {
        expect(SYSTEM.length).toBeGreaterThan(0)
        const { result } = renderHook(() => usePermissionPanel([]))

        act(() => result.current.setTab("system"))
        expect(result.current.visibleGroups).toEqual(SYSTEM)
        expect(result.current.activeGroup).toBe(SYSTEM[0]?.name)

        act(() => result.current.setTab("business"))
        expect(result.current.visibleGroups).toEqual(BUSINESS)
        expect(result.current.activeGroup).toBe(BUSINESS[0]?.name)
    })

    it("keeps the manually selected group until it disappears from results", () => {
        expect(BUSINESS.length).toBeGreaterThan(1)
        const { result } = renderHook(() => usePermissionPanel([]))

        act(() => result.current.setActiveGroup(BUSINESS[1]!.name))
        expect(result.current.activeGroup).toBe(BUSINESS[1]!.name)

        const needle = BUSINESS[0]!.rows[0]!.codes[0]!
        const expected = filterMatrixByKeyword(BUSINESS, needle.toLowerCase())
        act(() => result.current.setKeyword(needle))
        expect(result.current.visibleGroups).toEqual(expected)
        if (expected.some((group) => group.name === BUSINESS[1]!.name)) {
            expect(result.current.activeGroup).toBe(BUSINESS[1]!.name)
        } else {
            expect(result.current.activeGroup).toBe(expected[0]?.name ?? null)
        }
    })

    it("reports per-group progress against the full catalog", () => {
        const group = BUSINESS[0]!
        const codes = group.codes.slice(0, 2)

        const { result } = renderHook(() => usePermissionPanel(codes))
        const progress = result.current.progressByGroup.find(
            (item) => item.name === group.name,
        )

        expect(progress).toEqual({
            name: group.name,
            selected: codes.length,
            total: group.codes.length,
        })
    })

    it("recomputes per-tab selection counts and ignores unknown codes", () => {
        const businessCode = BUSINESS[0]?.codes[0]
        const systemCode = SYSTEM[0]?.codes[0]
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
