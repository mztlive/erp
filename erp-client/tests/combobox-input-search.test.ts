import { renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import {
    remoteSearchFromInputChange,
    useStickySelected,
} from "@/components/business/combobox-input-search"

describe("remoteSearchFromInputChange", () => {
    it("treats typing and clearing as a real search", () => {
        expect(remoteSearchFromInputChange("胶水", "input-change")).toBe("胶水")
        expect(remoteSearchFromInputChange("", "input-change")).toBe("")
        expect(remoteSearchFromInputChange("", "input-clear")).toBe("")
        expect(remoteSearchFromInputChange("", "clear-press")).toBe("")
    })

    it("resets the query on item press and ignores selected-label echo", () => {
        expect(remoteSearchFromInputChange("某某公司（简称）", "item-press")).toBe(
            "",
        )
        expect(remoteSearchFromInputChange("某某公司（简称）", "none")).toBe(
            undefined,
        )
        expect(remoteSearchFromInputChange("某某公司（简称）", "focus-out")).toBe(
            undefined,
        )
    })
})

describe("useStickySelected", () => {
    const keyOf = (item: { id: string }) => item.id

    it("returns the list item and keeps it after the list drops it", () => {
        const first = { id: "c1", name: "甲" }
        const { result, rerender } = renderHook(
            ({ items, key }) => useStickySelected(items, key, keyOf),
            { initialProps: { items: [first], key: "c1" } },
        )
        expect(result.current).toBe(first)

        rerender({ items: [{ id: "c2", name: "乙" }], key: "c1" })
        expect(result.current).toEqual(first)
    })

    it("clears the sticky item when the selection is cleared", () => {
        const first = { id: "c1", name: "甲" }
        const { result, rerender } = renderHook(
            ({ items, key }: { items: { id: string; name: string }[]; key?: string }) =>
                useStickySelected(items, key, keyOf),
            { initialProps: { items: [first], key: "c1" as string | undefined } },
        )

        rerender({ items: [first], key: undefined })
        expect(result.current).toBeNull()
    })
})
