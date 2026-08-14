import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useSupplierListState } from "./use-supplier-list-state"
import type {
    MasterDataListItem,
    MasterDataListResult,
} from "@/features/master-data/types"

const listMocks = vi.hoisted(() => ({
    data: null as MasterDataListResult | null,
    handleExport: vi.fn(),
}))

const filtersMocks = vi.hoisted(() => ({
    filters: {
        q: "",
        lifecycleStatus: "all" as "enabled" | "disabled" | "all",
        supplierCapabilityCodes: [] as string[],
        supplierQualificationTypes: [] as string[],
        supplierQualificationHealth: undefined as
            | "valid"
            | "expiring_30"
            | "expired"
            | "not_registered"
            | undefined,
        pagination: { pageIndex: 0, pageSize: 2 },
    },
}))

vi.mock("./use-supplier-list-filters", () => ({
    useSupplierListFilters: () => filtersMocks.filters,
}))

vi.mock("./use-create-permission", () => ({
    useCreatePermission: () => ({ canCreate: true, createBlockedReason: "" }),
}))

vi.mock("./queries", () => ({
    useMasterDataListQuery: () => ({
        isPending: false,
        isError: false,
        error: null,
        data: listMocks.data,
        refetch: vi.fn(),
    }),
}))

vi.mock("./use-master-data-list-export", () => ({
    useMasterDataListExport: () => ({
        exportMeta: null,
        handleExport: listMocks.handleExport,
    }),
}))

function makeItem(overrides: Partial<MasterDataListItem> = {}): MasterDataListItem {
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

function makeListResult(
    rows: readonly MasterDataListItem[],
): MasterDataListResult {
    return {
        resource: "suppliers",
        rows,
        totalCount: rows.length,
        permissionVersion: "p1",
        effectiveAsOf: "2026-06-01T00:00:00.000Z",
        eligibilityAsOf: "2026-06-01T00:00:00.000Z",
        queriedAt: "2026-06-01T00:00:00.000Z",
        metrics: [
            { key: "enabled", label: "启用", value: 99 },
            { key: "disabled", label: "停用", value: 99 },
            { key: "pending", label: "待生效", value: 99 },
            { key: "expiring", label: "即将到期", value: 99 },
        ],
    }
}

beforeEach(() => {
    listMocks.data = null
    listMocks.handleExport = vi.fn().mockResolvedValue(undefined)
})

describe("useSupplierListState", () => {
    it("resolves empty rows and passes the query through", () => {
        listMocks.data = makeListResult([])
        const { result } = renderHook(() =>
            useSupplierListState({ current: null }),
        )
        expect(result.current.rows).toEqual([])
        expect(result.current.pageRows).toEqual([])
        expect(result.current.canCreate).toBe(true)
        expect(result.current.syncedMetrics).toEqual(
            listMocks.data?.metrics,
        )
    })

    it("slices rows for the current page", () => {
        const rows = [
            makeItem({ stableId: "sup-1", lifecycleStatus: "ENABLED" }),
            makeItem({ stableId: "sup-2", lifecycleStatus: "DISABLED" }),
            makeItem({ stableId: "sup-3", lifecycleStatus: "ENABLED" }),
        ]
        listMocks.data = makeListResult(rows)
        const { result } = renderHook(() =>
            useSupplierListState({ current: null }),
        )
        expect(result.current.pageRows.map((row) => row.stableId)).toEqual([
            "sup-1",
            "sup-2",
        ])
    })

    it("syncs metric counts with rows and drops the pending metric", () => {
        const rows = [
            makeItem({ stableId: "sup-1", lifecycleStatus: "ENABLED" }),
            makeItem({ stableId: "sup-2", lifecycleStatus: "DISABLED" }),
            makeItem({
                stableId: "sup-3",
                lifecycleStatus: "ENABLED",
                revisionTiming: "FUTURE",
                revisionTimingLabel: "待生效",
                metricTags: ["expiring"],
            }),
        ]
        listMocks.data = makeListResult(rows)
        const { result } = renderHook(() =>
            useSupplierListState({ current: null }),
        )
        const byKey = new Map(
            result.current.syncedMetrics.map((metric) => [
                metric.key,
                metric.value,
            ]),
        )
        expect(byKey.get("enabled")).toBe(2)
        expect(byKey.get("disabled")).toBe(1)
        expect(byKey.get("expiring")).toBe(1)
        expect(byKey.has("pending")).toBe(false)
    })

    it("onExport re-queries with the active filters and a snapshot label", async () => {
        const rows = [
            makeItem({ stableId: "sup-1" }),
            makeItem({ stableId: "sup-2" }),
        ]
        listMocks.data = makeListResult(rows)
        filtersMocks.filters = {
            ...filtersMocks.filters,
            q: "茶叶",
            lifecycleStatus: "enabled",
        }
        const { result } = renderHook(() =>
            useSupplierListState({ current: null }),
        )
        await act(async () => {
            await result.current.onExport()
        })
        expect(listMocks.handleExport).toHaveBeenCalledTimes(1)
        const [query, label, fileLabel] = listMocks.handleExport.mock.calls[0]!
        expect(query).toMatchObject({
            resource: "suppliers",
            q: "茶叶",
            lifecycleStatus: "enabled",
        })
        expect(label).toContain("搜索=茶叶")
        expect(label).toContain("分类=供应商与资质")
        expect(fileLabel).toBe("供应商与资质")
    })

    it("onExport is a no-op without list data", async () => {
        const { result } = renderHook(() =>
            useSupplierListState({ current: null }),
        )
        await act(async () => {
            await result.current.onExport()
        })
        expect(listMocks.handleExport).not.toHaveBeenCalled()
    })
})
