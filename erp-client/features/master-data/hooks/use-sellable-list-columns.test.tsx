import { cleanup, render, renderHook } from "@testing-library/react"
import type { CellContext } from "@tanstack/react-table"
import type { ReactNode } from "react"
import { afterEach, describe, expect, it } from "vitest"

import { useSellableListColumns } from "./use-sellable-list-columns"
import type { MasterDataListItem } from "@/features/master-data/types"

afterEach(cleanup)

function makeRow(
    sellable: NonNullable<MasterDataListItem["sellableItem"]> | null,
): MasterDataListItem {
    return {
        objectType: "sellable-items",
        stableId: "sk1",
        stableNo: "SKU-001",
        name: "签字笔",
        lifecycleStatus: "ENABLED",
        lifecycleStatusLabel: "启用",
        lifecycleTone: "success",
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: "r1",
        displayedRevisionId: "r1",
        revisionNo: 1,
        effectiveFrom: "2026-01-01",
        keyFacts: [],
        selectorEligibility: [],
        allowedActions: [],
        actionBlockers: [],
        lockVersion: 1,
        metricTags: [],
        sellableItem: sellable ?? undefined,
    }
}

function makeSellable(
    overrides: Partial<NonNullable<MasterDataListItem["sellableItem"]>> = {},
): NonNullable<MasterDataListItem["sellableItem"]> {
    return {
        productId: "p1",
        productNo: "P-001",
        specificationAttributes: [{ name: "颜色", value: "红" }],
        specificationLabel: "颜色：红",
        baseUnit: "件",
        productKindLabel: "实物",
        salesVisiblePriceGross: "12.50",
        supplierCount: 3,
        supplyRegions: ["华东", "华南"],
        eligibilityAsOf: "2026-08-14",
        ...overrides,
    }
}

function renderCell(columnId: string, row: MasterDataListItem) {
    const { result } = renderHook(() => useSellableListColumns())
    const column = result.current.find((c) => c.id === columnId)
    expect(column).toBeDefined()
    const ctx = {
        row: { original: row },
    } as CellContext<MasterDataListItem, unknown>
    const cell = column!.cell as
        | ((props: CellContext<MasterDataListItem, unknown>) => ReactNode)
        | undefined
    return render(cell?.(ctx))
}

describe("useSellableListColumns", () => {
    it("returns the expected column ids in order", () => {
        const { result } = renderHook(() => useSellableListColumns())

        expect(result.current.map((c) => c.id)).toEqual([
            "name",
            "productNo",
            "price",
            "marketPrice",
            "supplyRegions",
            "supplierCount",
        ])
    })

    it("renders sku name with specification and sku number", () => {
        const cell = renderCell("name", makeRow(makeSellable()))

        expect(cell.getByText("签字笔")).toBeDefined()
        expect(cell.getByText(/颜色：红/)).toBeDefined()
        expect(cell.getByText("SKU-001")).toBeDefined()
    })

    it("renders the product number and sales price", () => {
        const productNo = renderCell("productNo", makeRow(makeSellable()))
        expect(productNo.getByText("P-001")).toBeDefined()

        const price = renderCell("price", makeRow(makeSellable()))
        expect(price.getByText("¥12.50")).toBeDefined()
        // 含税口径写在列头，不再逐行重复
        expect(price.queryByText("含税")).toBeNull()
    })

    it("declares the tax basis once in the price column header", () => {
        const { result } = renderHook(() => useSellableListColumns())
        const price = result.current.find((c) => c.id === "price")

        expect(price?.header).toBe("销售价（含税）")
    })

    it("uses the approved business column labels", () => {
        const { result } = renderHook(() => useSellableListColumns())
        const headers = Object.fromEntries(
            result.current.map((column) => [column.id, column.header]),
        )

        expect(headers.name).toBe("商品名称 · 规格")
        expect(headers.productNo).toBe("SPU 编号")
        expect(headers.marketPrice).toBe("市场参考价")
        expect(headers.supplierCount).toBe("供应保障")
    })

    it("right-aligns money and count columns with tabular numerals", () => {
        const { result } = renderHook(() => useSellableListColumns())

        for (const id of ["price", "marketPrice", "supplierCount"]) {
            const meta = result.current.find((c) => c.id === id)?.meta as
                | { align?: string; numeric?: boolean }
                | undefined
            expect(meta?.align).toBe("end")
            expect(meta?.numeric).toBe(true)
        }
    })

    it("lets the identity column absorb the row remainder", () => {
        const { result } = renderHook(() => useSellableListColumns())
        const name = result.current.find((c) => c.id === "name")?.meta as
            | { width?: string }
            | undefined

        expect(name?.width).toBe("flex")
    })

    it("sorts money columns by decimal value, not string order", () => {
        const { result } = renderHook(() => useSellableListColumns())
        const price = result.current.find((c) => c.id === "price")
        const sortingFn = price?.sortingFn as unknown as (
            a: { getValue: () => unknown },
            b: { getValue: () => unknown },
            id: string,
        ) => number

        const row = (value: string) => ({ getValue: () => value })

        // 字符串序会把 '9.90' 排在 '12.50' 之后
        expect(sortingFn(row("9.90"), row("12.50"), "price")).toBe(-1)
        expect(sortingFn(row("12.50"), row("9.90"), "price")).toBe(1)
        // 缺值排最后
        expect(sortingFn(row(""), row("9.90"), "price")).toBe(1)
    })

    it("renders a dash for a missing product number", () => {
        const noProduct = renderCell("productNo", makeRow(null))
        expect(noProduct.getByText("—")).toBeDefined()
    })

    it("renders a dash for a missing market price", () => {
        const noMarket = renderCell(
            "marketPrice",
            makeRow(makeSellable({ marketPrice: undefined })),
        )
        expect(noMarket.getByText("—")).toBeDefined()
    })

    it("renders market price when present", () => {
        const cell = renderCell(
            "marketPrice",
            makeRow(makeSellable({ marketPrice: "9.90" })),
        )
        expect(cell.getByText("¥9.90")).toBeDefined()
    })

    it("renders supply regions as compact badges and shows 未标注 when empty", () => {
        const joined = renderCell("supplyRegions", makeRow(makeSellable()))
        expect(joined.getByText("华东")).toBeDefined()
        expect(joined.getByText("华南")).toBeDefined()

        const empty = renderCell(
            "supplyRegions",
            makeRow(makeSellable({ supplyRegions: [] })),
        )
        expect(empty.getByText("未标注")).toBeDefined()
    })

    it("collapses long region lists and keeps the full value in the title", () => {
        const cell = renderCell(
            "supplyRegions",
            makeRow(
                makeSellable({
                    supplyRegions: ["华东", "华南", "西南", "华北"],
                }),
            ),
        )

        expect(cell.getByText("+2")).toBeDefined()
        expect(cell.getByTitle("华东、华南、西南、华北")).toBeDefined()
    })

    it("renders healthy supply as a success badge", () => {
        const cell = renderCell("supplierCount", makeRow(makeSellable()))

        expect(cell.getByText("3")).toBeDefined()
        expect(cell.container.textContent).toContain("家可供")
        expect(
            cell.container.querySelector('[data-variant="success"]'),
        ).not.toBeNull()
    })

    it("flags a sku served by a single supplier as a supply risk", () => {
        // 同一个 test 里两次 render 共用 document.body，断言必须锁在各自 container 上
        const risky = renderCell(
            "supplierCount",
            makeRow(makeSellable({ supplierCount: 1 })),
        )
        expect(risky.container.textContent).toContain("单一供应商")
        expect(risky.container.querySelector("svg")).not.toBeNull()

        cleanup()

        const safe = renderCell(
            "supplierCount",
            makeRow(makeSellable({ supplierCount: 2 })),
        )
        expect(safe.container.textContent).not.toContain("单一供应商")
        expect(safe.container.querySelector("svg")).toBeNull()
    })
})
