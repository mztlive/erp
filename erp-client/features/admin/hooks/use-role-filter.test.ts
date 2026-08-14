import { act, renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { useRoleFilter } from "./use-role-filter"

const options = [
    { id: "role-ops", name: "运营" },
    { id: "role-sales", name: "销售" },
]

describe("useRoleFilter", () => {
    it("returns all options untouched when the keyword is empty", () => {
        const { result } = renderHook(() => useRoleFilter(options))

        expect(result.current.keyword).toBe("")
        expect(result.current.filtered).toBe(options)
    })

    it("filters by name or id, trimmed and case-insensitive", () => {
        const { result } = renderHook(() => useRoleFilter(options))

        act(() => result.current.setKeyword(" 销售 "))
        expect(result.current.filtered.map((role) => role.id)).toEqual([
            "role-sales",
        ])

        act(() => result.current.setKeyword("ROLE-OPS"))
        expect(result.current.filtered.map((role) => role.id)).toEqual([
            "role-ops",
        ])
    })

    it("returns an empty list when nothing matches", () => {
        const { result } = renderHook(() => useRoleFilter(options))

        act(() => result.current.setKeyword("不存在"))
        expect(result.current.filtered).toEqual([])
    })

    it("restores the original reference after clearing the keyword", () => {
        const { result } = renderHook(() => useRoleFilter(options))

        act(() => result.current.setKeyword("销售"))
        expect(result.current.filtered).not.toBe(options)

        act(() => result.current.setKeyword(""))
        expect(result.current.filtered).toBe(options)
    })

    it("handles empty option lists", () => {
        const { result } = renderHook(() => useRoleFilter([]))

        expect(result.current.filtered).toEqual([])
        act(() => result.current.setKeyword("任意"))
        expect(result.current.filtered).toEqual([])
    })
})
