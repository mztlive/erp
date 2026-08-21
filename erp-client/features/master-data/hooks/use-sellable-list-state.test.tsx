import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { act, waitFor } from "@testing-library/react"

import { useSellableListState } from "./use-sellable-list-state"
import type {
    MasterDataListItem,
    MasterDataListResult,
} from "@/features/master-data/types"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

const stateMocks = vi.hoisted(() => ({
    listData: null as MasterDataListResult | null,
    listPending: false,
    listError: null as unknown,
    filterOptions: {
        categories: [] as {
            categoryId: string
            categoryCode: string
            categoryName: string
            parentId?: string
        }[],
        brands: [] as { value: string; label: string; keywords: string }[],
        suppliers: [] as { value: string; label: string; keywords: string }[],
    },
    exportMutateAsync: vi.fn(),
}))

let currentSearchParams = new URLSearchParams()

vi.mock("next/navigation", () => ({
    useRouter: () => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() }),
    usePathname: () => "/master-data/sellable-items",
    useSearchParams: () => currentSearchParams,
    useParams: () => ({}),
}))

vi.mock("@/features/master-data/hooks/queries", () => ({
    useMasterDataListQuery: () => ({
        data: stateMocks.listData,
        isPending: stateMocks.listPending,
        isError: stateMocks.listError != null,
        error: stateMocks.listError,
        refetch: vi.fn(),
    }),
    useProductFilterOptionsQuery: () => ({
        data: stateMocks.filterOptions,
        isPending: false,
        isError: false,
        error: null,
    }),
    useMasterDataExportMutation: () => ({
        mutateAsync: stateMocks.exportMutateAsync,
    }),
}))

function makeRow(
    overrides: Partial<MasterDataListItem> = {},
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
        sellableItem: {
            productId: "p1",
            productNo: "P-001",
            specificationAttributes: [{ name: "颜色", value: "红" }],
            specificationLabel: "颜色：红",
            baseUnit: "件",
            productKindLabel: "实物",
            salesVisiblePriceGross: "12.50",
            supplierCount: 2,
            supplyRegions: ["华东"],
            eligibilityAsOf: "2026-08-14",
        },
        ...overrides,
    }
}

function makeListResult(
    rows: readonly MasterDataListItem[],
): MasterDataListResult {
    return {
        resource: "sellable-items",
        rows,
        totalCount: rows.length,
        permissionVersion: "v1",
        effectiveAsOf: "2026-08-14",
        eligibilityAsOf: "2026-08-14",
        queriedAt: "2026-08-14T00:00:00.000Z",
        metrics: [],
    }
}

function renderState() {
    const searchInputRef = { current: null as HTMLInputElement | null }
    return renderHookWithProviders(() => useSellableListState(searchInputRef), {
        queryClient: createFreshQueryClient(),
    })
}

describe("useSellableListState", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        stateMocks.listData = null
        stateMocks.listPending = false
        stateMocks.listError = null
        currentSearchParams = new URLSearchParams()
        URL.createObjectURL = vi.fn(() => "blob:csv")
        URL.revokeObjectURL = vi.fn()
    })

    afterEach(() => {
        vi.restoreAllMocks()
    })

    it("keeps the preview closed by default and resolves the preview row", () => {
        const row = makeRow()
        stateMocks.listData = makeListResult([row])
        const { result } = renderState()

        expect(result.current.previewId).toBeNull()
        expect(result.current.previewRow).toBeNull()

        act(() => result.current.setPreviewId("sk1"))
        expect(result.current.previewRow?.stableId).toBe("sk1")

        act(() => result.current.setPreviewId("missing"))
        expect(result.current.previewRow).toBeNull()
    })

    it("hands the full result set to the table for sorting and paging", () => {
        const rows = Array.from({ length: 45 }, (_, i) =>
            makeRow({ stableId: `sk${i}`, stableNo: `SKU-${i}` }),
        )
        stateMocks.listData = makeListResult(rows)
        const { result } = renderState()

        // 分页交给 DataTable：排序必须先作用于整份结果，再切页
        expect(result.current.rows).toHaveLength(45)
        expect("pageRows" in result.current).toBe(false)
    })

    it("builds the table description from rows and filters", () => {
        stateMocks.listData = makeListResult([makeRow()])
        const { result } = renderState()

        expect(result.current.sellableTableDescription).toContain(
            "点击任一行查看",
        )
    })

    it("builds shortcut counts and derives the visible rows", () => {
        stateMocks.listData = makeListResult([
            makeRow({
                stableId: "one",
                sellableItem: {
                    ...makeRow().sellableItem!,
                    supplierCount: 1,
                },
            }),
            makeRow({
                stableId: "national",
                sellableItem: {
                    ...makeRow().sellableItem!,
                    supplierCount: 3,
                    supplyRegions: ["全国"],
                },
            }),
            makeRow({
                stableId: "regional",
                sellableItem: {
                    ...makeRow().sellableItem!,
                    supplierCount: 2,
                    supplyRegions: ["华南"],
                },
            }),
        ])
        currentSearchParams = new URLSearchParams(
            "supplyPreset=single-supplier",
        )

        const { result } = renderState()

        expect(result.current.supplyPresetCounts).toEqual({
            all: 3,
            "single-supplier": 1,
            nationwide: 1,
        })
        expect(result.current.rows.map((row) => row.stableId)).toEqual(["one"])
        expect(result.current.appliedChips).toEqual([
            { key: "supplyPreset", label: "单一供应商" },
        ])
    })

    it("exports nothing without loaded rows", async () => {
        const { result } = renderState()

        await act(async () => {
            await result.current.onExport()
        })

        expect(stateMocks.exportMutateAsync).not.toHaveBeenCalled()
    })

    it("exports sellable rows via a fresh server query", async () => {
        stateMocks.listData = makeListResult([
            makeRow({
                sellableItem: {
                    ...makeRow().sellableItem!,
                    supplierCount: 1,
                },
            }),
        ])
        stateMocks.exportMutateAsync.mockResolvedValue(
            makeListResult([makeRow()]),
        )
        const { result } = renderState()

        await act(async () => {
            await result.current.onExport()
        })

        expect(stateMocks.exportMutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({ resource: "sellable-items" }),
        )
        await waitFor(() => expect(result.current.exportMeta).not.toBeNull())
    })

    it("exports the same shortcut view shown in the table", async () => {
        stateMocks.listData = makeListResult([
            makeRow({
                sellableItem: {
                    ...makeRow().sellableItem!,
                    supplierCount: 1,
                },
            }),
        ])
        stateMocks.exportMutateAsync.mockResolvedValue(
            makeListResult([makeRow()]),
        )
        currentSearchParams = new URLSearchParams(
            "supplyPreset=single-supplier",
        )
        const { result } = renderState()

        await act(async () => {
            await result.current.onExport()
        })

        expect(stateMocks.exportMutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                resource: "sellable-items",
                sellableSupplyPreset: "single-supplier",
            }),
        )
    })
})
