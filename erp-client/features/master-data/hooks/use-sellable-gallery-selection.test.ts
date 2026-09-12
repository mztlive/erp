import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import type { MasterDataListItem } from "@/features/master-data/types"
import { useSellableGallerySelection } from "./use-sellable-gallery-selection"

afterEach(cleanup)

function row(id: string, name = id): MasterDataListItem {
    return {
        objectType: "sellable-items",
        stableId: id,
        stableNo: id,
        name,
        lifecycleStatus: "ENABLED",
        lifecycleStatusLabel: "当前启用",
        lifecycleTone: "success",
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: `rev-${id}`,
        displayedRevisionId: `rev-${id}`,
        revisionNo: 1,
        effectiveFrom: "2026-01-01",
        keyFacts: [],
        selectorEligibility: [],
        allowedActions: [],
        actionBlockers: [],
        lockVersion: 1,
        metricTags: ["enabled"],
        sellableItem: {
            productId: "p-1",
            productNo: "P-1",
            specificationAttributes: [],
            specificationLabel: "无规格",
            baseUnit: "件",
            productKindLabel: "实物",
            salesVisiblePriceGross: "1.00",
            supplierCount: 1,
            supplyRegions: [],
            eligibilityAsOf: "2026-08-25",
        },
    }
}

describe("useSellableGallerySelection", () => {
    it("shares table and gallery selection using stable SKU ids", () => {
        const { result, rerender } = renderHook(
            ({ rows }) => useSellableGallerySelection(rows),
            { initialProps: { rows: [row("sku-1"), row("sku-2")] } },
        )
        act(() => result.current.onRowSelectionChange({ "sku-2": true }))
        expect([...result.current.selectedIds]).toEqual(["sku-2"])
        expect(
            result.current.selectedRows.map((item) => item.stableId),
        ).toEqual(["sku-2"])
        act(() => result.current.toggle("sku-1", true))
        expect(result.current.rowSelection).toEqual({
            "sku-1": true,
            "sku-2": true,
        })
        rerender({ rows: [row("sku-2"), row("sku-1")] })
        expect(result.current.allSelected).toBe(true)
        act(() =>
            result.current.onRowSelectionChange({
                "sku-1": false,
                "sku-2": true,
                missing: true,
            }),
        )
        expect(result.current.rowSelection).toEqual({ "sku-2": true })
        act(() => result.current.clear())
        expect(result.current.rowSelection).toEqual({})
        expect(result.current.selectedCount).toBe(0)
    })

    it("selects all current results and drops ids that leave the result set", () => {
        const first = [row("sku-1"), row("sku-2")]
        const { result, rerender } = renderHook(
            ({ rows }) => useSellableGallerySelection(rows),
            { initialProps: { rows: first } },
        )
        act(() => {
            result.current.selectAllResults()
        })
        expect(result.current.selectedCount).toBe(2)
        expect(result.current.allSelected).toBe(true)
        rerender({ rows: [row("sku-2"), row("sku-3")] })
        expect([...result.current.selectedIds]).toEqual(["sku-2"])
        expect(result.current.someSelected).toBe(true)
    })
})
