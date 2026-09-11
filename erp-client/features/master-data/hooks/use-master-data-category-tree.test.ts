import * as React from "react"
import { act, cleanup, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { mapCategoryRow } from "@/features/master-data/api/list-mappers"
import { useMasterDataCategoryTree } from "./use-master-data-category-tree"

const mocks = vi.hoisted(() => ({
    params: new URLSearchParams(),
    patch: vi.fn(),
    pending: false,
}))
const rows = [
    mapCategoryRow({
        id: "root",
        category_code: "FOOD",
        name: "食品",
        parent_category_id: null,
        product_kind: "PHYSICAL",
        status: "active",
        version: 1,
        created_at: 1,
    }),
    mapCategoryRow({
        id: "tea",
        category_code: "TEA",
        name: "茶叶",
        parent_category_id: "root",
        product_kind: "PHYSICAL",
        status: "disabled",
        version: 1,
        created_at: 1,
    }),
    mapCategoryRow({
        id: "green",
        category_code: "GREEN",
        name: "绿茶",
        parent_category_id: "tea",
        product_kind: "PHYSICAL",
        status: "active",
        version: 1,
        created_at: 1,
    }),
]
vi.mock("./use-list-url", () => ({
    useListUrl: () => ({
        searchParams: mocks.params,
        patchUrl: mocks.patch,
        q: mocks.params.get("q") ?? "",
    }),
    useSearchDraft: (q: string) => {
        const [searchDraft, setSearchDraft] = React.useState(q)
        return { searchDraft, setSearchDraft }
    },
}))
vi.mock("./queries", () => ({
    useMasterDataListQuery: (query: {
        q?: string
        lifecycleStatus: string
    }) => ({
        data: {
            rows: query.q
                ? [rows[2]]
                : query.lifecycleStatus === "disabled"
                  ? [rows[1]]
                  : rows,
        },
        isPlaceholderData: Boolean(query.q) && mocks.pending,
    }),
}))
vi.mock("./use-create-permission", () => ({
    useCreatePermission: () => ({ canCreate: true, createBlockedReason: "" }),
}))
vi.mock("./use-slash-search-hotkey", () => ({ useSlashSearchHotkey: vi.fn() }))
vi.mock("@/features/master-data/lib/export-csv", () => ({
    buildMasterDataExportCsv: vi.fn(),
    downloadCsv: vi.fn(),
}))
const useTree = () =>
    useMasterDataCategoryTree(React.useRef<HTMLInputElement | null>(null))
beforeEach(() => {
    mocks.params = new URLSearchParams()
    mocks.patch.mockReset()
    mocks.pending = false
    sessionStorage.clear()
})
afterEach(cleanup)

describe("category navigation", () => {
    it("keeps the complete ancestor path while exporting only matches", () => {
        mocks.params = new URLSearchParams("q=绿茶&category=green")
        const { result } = renderHook(useTree)
        expect(result.current.forest[0].item.stableId).toBe("root")
        expect(result.current.forest[0].children[0].children[0].pathLabel).toBe(
            "食品 / 茶叶 / 绿茶",
        )
        expect(result.current.matchedRows.map((row) => row.stableId)).toEqual([
            "green",
        ])
        expect(result.current.selectedNode?.pathLabel).toBe(
            "食品 / 茶叶 / 绿茶",
        )
    })
    it("keeps selected details when a status filter hides the selected node", () => {
        mocks.params = new URLSearchParams(
            "lifecycleStatus=disabled&category=green",
        )
        const { result } = renderHook(useTree)
        expect(result.current.selected?.name).toBe("绿茶")
        expect(result.current.matchedIds.has("green")).toBe(false)
        expect(result.current.forest[0].children[0].item.name).toBe("茶叶")
    })
    it("keeps collapse-all collapsed and restores the unfiltered expansion after search", () => {
        const { result, rerender } = renderHook(useTree)
        act(() => result.current.collapseAll())
        expect(result.current.expanded.size).toBe(0)
        mocks.params = new URLSearchParams("q=绿茶")
        rerender()
        expect(result.current.expanded.has("tea")).toBe(true)
        mocks.params = new URLSearchParams()
        rerender()
        expect(result.current.expanded.size).toBe(0)
    })
    it("does not label previous query rows as current results", () => {
        mocks.params = new URLSearchParams("q=绿茶")
        mocks.pending = true
        const { result } = renderHook(useTree)
        expect(result.current.matchedRows).toEqual([])
        expect(result.current.rows).toHaveLength(3)
    })
    it("selects a new child and expands its ancestry", () => {
        const { result } = renderHook(useTree)
        act(() => result.current.collapseAll())
        act(() => result.current.openCreateChild(rows[1]))
        act(() => result.current.onCreated("new-child"))
        expect(mocks.patch).toHaveBeenLastCalledWith({
            category: "new-child",
            q: null,
            lifecycleStatus: null,
        })
        expect(result.current.expanded.has("root")).toBe(true)
        expect(result.current.expanded.has("tea")).toBe(true)
    })
})
