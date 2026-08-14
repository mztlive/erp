import { describe, it, expect, vi } from "vitest"
import { renderHook } from "@testing-library/react"

import { useSupplierListColumns } from "./use-supplier-list-columns"
import type { MasterDataListItem } from "@/features/master-data/types"

function makeItem(
    overrides: Partial<MasterDataListItem> = {},
): MasterDataListItem {
    return {
        objectType: "suppliers",
        stableId: "sup-1",
        stableNo: "S-001",
        name: "示例供应商",
        lifecycleStatus: "ENABLED",
        lifecycleStatusLabel: "启用",
        lifecycleTone: "success",
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: "rev-1",
        displayedRevisionId: "rev-1",
        revisionNo: 1,
        effectiveFrom: "2026-06-01T00:00:00.000Z",
        keyFacts: [],
        selectorEligibility: [],
        allowedActions: ["CREATE_REVISION", "DISABLE"],
        actionBlockers: [],
        lockVersion: 1,
        metricTags: [],
        ...overrides,
    }
}

describe("useSupplierListColumns", () => {
    it("builds the fixed supplier columns with the actions column last", () => {
        const lastFocusedRowId = { current: null }
        const onOpen = vi.fn()
        const onDisableTarget = vi.fn()
        const { result } = renderHook(() =>
            useSupplierListColumns({
                lastFocusedRowId,
                rows: [],
                onOpen,
                onDisableTarget,
            }),
        )
        expect(result.current.map((column) => column.id)).toEqual([
            "name",
            "revisionNo",
            "lifecycle",
            "revisionTiming",
            "actions",
        ])
    })

    it("adds the blocker column only when some row has a primary blocker", () => {
        const { result } = renderHook(() =>
            useSupplierListColumns({
                lastFocusedRowId: { current: null },
                rows: [
                    makeItem({ primaryBlocker: "资质已过期" }),
                    makeItem({ stableId: "sup-2" }),
                ],
                onOpen: vi.fn(),
                onDisableTarget: vi.fn(),
            }),
        )
        expect(result.current.map((column) => column.id)).toEqual([
            "name",
            "revisionNo",
            "lifecycle",
            "revisionTiming",
            "blocker",
            "actions",
        ])
    })
})
