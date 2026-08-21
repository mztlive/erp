import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useMasterDataCategoryTree } from "./use-master-data-category-tree"
import type { MasterDataListItem } from "@/features/master-data/types"

const queryMocks = vi.hoisted(() => ({
    useMasterDataListQuery: vi.fn(),
}))

vi.mock("@/features/master-data/hooks/queries", () => ({
    useMasterDataListQuery: queryMocks.useMasterDataListQuery,
}))

const navMocks = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: navMocks.back,
    }),
    useSearchParams: () => navMocks.searchParams,
    usePathname: () => "/master-data/categories",
    useParams: () => ({}),
}))

const csvMocks = vi.hoisted(() => ({
    buildMasterDataExportCsv: vi.fn(),
    downloadCsv: vi.fn(),
}))

vi.mock("@/features/master-data/lib/export-csv", () => ({
    buildMasterDataExportCsv: csvMocks.buildMasterDataExportCsv,
    downloadCsv: csvMocks.downloadCsv,
}))

function makeRow(
    stableId: string,
    overrides: Partial<MasterDataListItem> = {},
): MasterDataListItem {
    return {
        objectType: "categories",
        stableId,
        stableNo: `CAT-${stableId}`,
        name: stableId,
        lifecycleStatus: "ENABLED",
        lifecycleStatusLabel: "当前启用",
        lifecycleTone: "success",
        revisionTiming: "CURRENT",
        revisionTimingLabel: "当前生效",
        currentRevisionId: `rev-${stableId}`,
        displayedRevisionId: `rev-${stableId}`,
        revisionNo: 1,
        effectiveFrom: "2026-01-01",
        keyFacts: [],
        selectorEligibility: [],
        allowedActions: [],
        actionBlockers: [],
        lockVersion: 1,
        metricTags: [],
        ...overrides,
    }
}

// 名称按 zh-CN 排序：乙类在前，甲类在后；甲类下挂「甲一」。
const rows: MasterDataListItem[] = [
    makeRow("root-a", {
        name: "甲类",
        dictionaryCode: "A",
        parentStableId: undefined,
    }),
    makeRow("root-b", {
        name: "乙类",
        dictionaryCode: "B",
        parentStableId: undefined,
    }),
    makeRow("child-a1", {
        name: "甲一",
        dictionaryCode: "A1",
        parentStableId: "root-a",
    }),
]

function listQueryState() {
    return {
        data: {
            rows,
            resource: "categories" as const,
            totalCount: rows.length,
            permissionVersion: "1",
            effectiveAsOf: "2026-01-01",
            eligibilityAsOf: "2026-01-01",
            queriedAt: "2026-08-14T00:00:00.000Z",
            metrics: [],
        },
        isPending: false,
        isError: false,
        error: null,
        refetch: vi.fn(),
    }
}

beforeEach(() => {
    navMocks.searchParams = new URLSearchParams()
    navMocks.replace.mockClear()
    queryMocks.useMasterDataListQuery.mockReset()
    queryMocks.useMasterDataListQuery.mockReturnValue(listQueryState())
    csvMocks.buildMasterDataExportCsv.mockReset()
    csvMocks.downloadCsv.mockReset()
})

const treeSearchInputRef = { current: null as HTMLInputElement | null }

function renderTree() {
    return renderHook(() =>
        useMasterDataCategoryTree(treeSearchInputRef),
    )
}

describe("useMasterDataCategoryTree", () => {
    it("queries categories with the URL applied search and lifecycle state", () => {
        navMocks.searchParams = new URLSearchParams(
            "q=甲&lifecycleStatus=disabled",
        )
        renderTree()
        expect(queryMocks.useMasterDataListQuery).toHaveBeenCalledWith({
            resource: "categories",
            q: "甲",
            lifecycleStatus: "disabled",
            revisionTiming: "all",
        })
    })

    it("queries with defaults when the URL carries no filters", () => {
        renderTree()
        expect(queryMocks.useMasterDataListQuery).toHaveBeenCalledWith({
            resource: "categories",
            q: undefined,
            lifecycleStatus: "all",
            revisionTiming: "all",
        })
    })

    it("builds the forest sorted by name and expands roots on first load", () => {
        const { result } = renderTree()
        expect(result.current.forest.map((n) => n.item.name)).toEqual([
            "甲类",
            "乙类",
        ])
        expect(result.current.flat).toHaveLength(3)
        expect(result.current.expanded.has("root-a")).toBe(true)
        expect(result.current.expanded.has("root-b")).toBe(true)
    })

    it("counts only expanded descendants as visible", () => {
        const { result } = renderTree()
        // 乙类 + 甲类 + 甲一（甲类默认展开）
        expect(result.current.visibleCount).toBe(3)

        act(() => {
            result.current.toggle("root-a")
        })
        expect(result.current.visibleCount).toBe(2)

        act(() => {
            result.current.expandAll()
        })
        expect(result.current.visibleCount).toBe(3)

        // 清空后，首次加载副作用会把根重新展开（既有行为）。
        act(() => {
            result.current.collapseAll()
        })
        expect(result.current.visibleCount).toBe(3)
    })

    it("toggling a node expands and collapses it", () => {
        const { result } = renderTree()
        act(() => {
            result.current.toggle("root-b")
        })
        expect(result.current.expanded.has("root-b")).toBe(false)
        act(() => {
            result.current.toggle("root-b")
        })
        expect(result.current.expanded.has("root-b")).toBe(true)
    })

    it("resolves the selected row and its path label", () => {
        const { result } = renderTree()
        act(() => {
            result.current.setSelectedId("child-a1")
        })
        expect(result.current.selected?.stableId).toBe("child-a1")
        expect(result.current.selectedPath).toBe("甲类 / 甲一")
    })

    it("flags active filters from the URL applied state", () => {
        const { result } = renderTree()
        expect(result.current.filterActive).toBe(false)

        navMocks.searchParams = new URLSearchParams("q= 甲 ")
        const rerendered = renderTree()
        expect(rerendered.result.current.filterActive).toBe(true)

        navMocks.searchParams = new URLSearchParams(
            "lifecycleStatus=disabled",
        )
        const disabledOnly = renderTree()
        expect(disabledOnly.result.current.filterActive).toBe(true)
        expect(result.current.appliedChips).toEqual([])
    })

    it("applies the search draft to the URL on form submit", () => {
        const { result } = renderTree()
        act(() => {
            result.current.setSearchDraft("  甲  ")
        })
        act(() => {
            result.current.applyTreeFilters()
        })
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/categories?q=%E7%94%B2",
            { scroll: false },
        )
    })

    it("writes the lifecycle quick filter directly to the URL", () => {
        const { result } = renderTree()
        act(() => {
            result.current.setLifecycleStatus("enabled")
        })
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/categories?lifecycleStatus=enabled",
            { scroll: false },
        )
    })

    it("builds applied chips for active search and lifecycle filters", () => {
        navMocks.searchParams = new URLSearchParams(
            "q=甲&lifecycleStatus=disabled",
        )
        const { result } = renderTree()
        expect(result.current.appliedChips).toEqual([
            { key: "q", label: "搜索：甲" },
            { key: "lifecycleStatus", label: "启停：当前停用" },
        ])
    })

    it("removes a single applied chip", () => {
        navMocks.searchParams = new URLSearchParams(
            "q=甲&lifecycleStatus=disabled",
        )
        const { result } = renderTree()
        act(() => {
            result.current.removeFilter("q")
        })
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/categories?lifecycleStatus=disabled",
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe("")
    })

    it("clears search and lifecycle filters together", () => {
        navMocks.searchParams = new URLSearchParams(
            "q=甲&lifecycleStatus=enabled",
        )
        const { result } = renderTree()
        act(() => {
            result.current.setSearchDraft("甲")
            result.current.clearFilters()
        })
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/master-data/categories",
            { scroll: false },
        )
        expect(result.current.searchDraft).toBe("")
    })

    it("opens the create dialog as root or child with the parent id", () => {
        const { result } = renderTree()
        act(() => {
            result.current.openCreateRoot()
        })
        expect(result.current.createOpen).toBe(true)
        expect(result.current.createParentId).toBeUndefined()

        act(() => {
            result.current.openCreateChild(rows[0])
        })
        expect(result.current.createParentId).toBe("root-a")
    })

    it("exports the current rows and records the export meta", () => {
        csvMocks.buildMasterDataExportCsv.mockReturnValue("csv")
        const { result } = renderTree()

        act(() => {
            result.current.onExport()
        })

        expect(csvMocks.buildMasterDataExportCsv).toHaveBeenCalledWith(
            rows,
            "分类=商品分类树",
        )
        expect(csvMocks.downloadCsv).toHaveBeenCalledWith(
            "csv",
            "基础资料-商品分类",
        )
        expect(result.current.exportMeta).toMatchObject({ rowCount: 3 })
        expect(result.current.exportMeta?.jobId).toMatch(/^导出-\d{8}-/)
    })

    it("does not download when there are no rows", () => {
        queryMocks.useMasterDataListQuery.mockReturnValue({
            ...listQueryState(),
            data: { ...listQueryState().data, rows: [] },
        })
        const { result } = renderTree()

        act(() => {
            result.current.onExport()
        })

        expect(csvMocks.downloadCsv).not.toHaveBeenCalled()
        expect(result.current.exportMeta).toBeNull()
    })
})
