import { describe, expect, it } from "vitest"

import {
    describePoolSource,
    resolvePoolSourceKind,
} from "@/features/sales-selection/lib/pool-source"

describe("resolvePoolSourceKind", () => {
    it("uses current selection when any SKU is checked", () => {
        expect(resolvePoolSourceKind(1)).toBe("SELECTION")
        expect(resolvePoolSourceKind(12)).toBe("SELECTION")
    })

    it("falls back to the current filter when nothing is checked", () => {
        expect(resolvePoolSourceKind(0)).toBe("FILTER")
    })
})

describe("describePoolSource", () => {
    it("summarizes checked items without exposing identities", () => {
        expect(
            describePoolSource({
                kind: "SELECTION",
                itemCount: 12,
                filterLabel: "全国可供",
            }),
        ).toBe("已勾选 12 件")
    })

    it("joins the active filter label with the result count", () => {
        expect(
            describePoolSource({
                kind: "FILTER",
                itemCount: 128,
                filterLabel: "全国可供 · 类型：实物",
            }),
        ).toBe("当前筛选 · 全国可供 · 类型：实物 · 128 件")
    })

    it("treats an empty filter as the full sellable pool", () => {
        expect(
            describePoolSource({
                kind: "FILTER",
                itemCount: 40,
                filterLabel: "  ",
            }),
        ).toBe("当前筛选 · 全部可售 · 40 件")
    })
})
