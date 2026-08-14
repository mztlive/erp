import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useDictionaryListState } from "./use-dictionary-list-state"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { MasterDataListItem } from "@/features/master-data/types"

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
    usePathname: () => "/master-data/unit-of-measures",
    useParams: () => ({}),
}))

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: vi.fn(),
}))

const queryMocks = vi.hoisted(() => ({
    useMasterDataListQuery: vi.fn(),
    useMasterDataCenterQuery: vi.fn(),
}))

vi.mock("@/features/master-data/hooks/queries", () => ({
    useMasterDataListQuery: queryMocks.useMasterDataListQuery,
    useMasterDataCenterQuery: queryMocks.useMasterDataCenterQuery,
}))

const exportMocks = vi.hoisted(() => ({
    handleExport: vi.fn(),
}))

vi.mock(
    "@/features/master-data/hooks/use-master-data-list-export",
    () => ({
        useMasterDataListExport: () => ({
            exportMutation: { isPending: false },
            exportMeta: null,
            handleExport: exportMocks.handleExport,
        }),
    }),
)

const mockedAccountQuery = vi.mocked(useAccountProfileQuery)

function makeRow(
    stableId: string,
    overrides: Partial<MasterDataListItem> = {},
): MasterDataListItem {
    return {
        objectType: "unit-of-measures",
        stableId,
        stableNo: `UOM-${stableId}`,
        name: `单位 ${stableId}`,
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

function listData(rows: readonly MasterDataListItem[]) {
    return {
        rows,
        resource: "unit-of-measures" as const,
        totalCount: rows.length,
        permissionVersion: "1",
        effectiveAsOf: "2026-01-01",
        eligibilityAsOf: "2026-01-01",
        queriedAt: "2026-08-14T00:00:00.000Z",
        metrics: [
            { key: "all", label: "全部", value: rows.length, detail: "当前分类" },
            {
                key: "enabled",
                label: "当前启用",
                value: rows.filter((r) => r.lifecycleStatus === "ENABLED")
                    .length,
                detail: "启用状态",
            },
            {
                key: "disabled",
                label: "当前停用",
                value: rows.filter((r) => r.lifecycleStatus === "DISABLED")
                    .length,
                detail: "历史保留",
            },
            {
                key: "pending",
                label: "待生效更新",
                value: rows.filter((r) => r.revisionTiming === "FUTURE").length,
                detail: "版本状态 · 不是启用状态",
            },
        ] as const,
    }
}

function listQueryState(data?: ReturnType<typeof listData>) {
    return {
        data,
        isPending: false,
        isError: false,
        error: null,
        refetch: vi.fn(),
    }
}

beforeEach(() => {
    navMocks.searchParams = new URLSearchParams()
    navMocks.replace.mockClear()
    exportMocks.handleExport.mockReset()
    queryMocks.useMasterDataListQuery.mockReset()
    queryMocks.useMasterDataCenterQuery.mockReset()
    mockedAccountQuery.mockReturnValue({
        data: {
            userid: "u1",
            account: "acct",
            name: "张三",
            subject: "s1",
            role_ids: [],
            permissions: [],
            account_kind: "staff",
        },
        isPending: false,
        isError: false,
        error: null,
    } as unknown as ReturnType<typeof useAccountProfileQuery>)
})

function renderState(overrides: { enablePreview?: boolean } = {}) {
    return renderHook(() =>
        useDictionaryListState({
            resource: "unit-of-measures",
            createPermission: "unit_of_measure:create",
            enablePreview: overrides.enablePreview ?? false,
            searchInputRef: { current: null },
        }),
    )
}

describe("useDictionaryListState", () => {
    it("passes URL filters into the list query with a trimmed q", () => {
        navMocks.searchParams = new URLSearchParams(
            "q=  箱  &lifecycleStatus=enabled&revisionTiming=future",
        )
        queryMocks.useMasterDataListQuery.mockReturnValue(listQueryState())
        queryMocks.useMasterDataCenterQuery.mockReturnValue({
            data: undefined,
            isPending: false,
        })

        renderState()

        expect(queryMocks.useMasterDataListQuery).toHaveBeenCalledWith({
            resource: "unit-of-measures",
            q: "箱",
            lifecycleStatus: "enabled",
            revisionTiming: "future",
        })
    })

    it("exposes rows and the client-paged page slice", () => {
        const rows = Array.from({ length: 25 }, (_, index) =>
            makeRow(`u-${index}`),
        )
        queryMocks.useMasterDataListQuery.mockReturnValue(
            listQueryState(listData(rows)),
        )
        queryMocks.useMasterDataCenterQuery.mockReturnValue({
            data: undefined,
            isPending: false,
        })

        const { result } = renderState()

        expect(result.current.rows).toHaveLength(25)
        expect(result.current.pageRows).toHaveLength(20)
        expect(result.current.pageRows[0].stableId).toBe("u-0")
    })

    it("syncs the metrics with the current rows", () => {
        const rows = [
            makeRow("a"),
            makeRow("b"),
            makeRow("c", { lifecycleStatus: "DISABLED", lifecycleStatusLabel: "当前停用", lifecycleTone: "neutral" }),
            makeRow("d", { revisionTiming: "FUTURE", revisionTimingLabel: "待生效" }),
        ]
        queryMocks.useMasterDataListQuery.mockReturnValue(
            listQueryState(listData(rows)),
        )
        queryMocks.useMasterDataCenterQuery.mockReturnValue({
            data: undefined,
            isPending: false,
        })

        const { result } = renderState()

        const byKey = Object.fromEntries(
            result.current.syncedMetrics.map((m) => [m.key, m.value]),
        )
        expect(byKey.all).toBe(4)
        expect(byKey.enabled).toBe(3)
        expect(byKey.disabled).toBe(1)
        expect(byKey.pending).toBe(1)
    })

    it("keeps the server metrics when there are no rows", () => {
        const base = listData([])
        queryMocks.useMasterDataListQuery.mockReturnValue(listQueryState(base))
        queryMocks.useMasterDataCenterQuery.mockReturnValue({
            data: undefined,
            isPending: false,
        })

        const { result } = renderState()
        expect(result.current.syncedMetrics).toEqual([...base.metrics])
    })

    it("disables the center query when preview is off", () => {
        queryMocks.useMasterDataListQuery.mockReturnValue(
            listQueryState(listData([makeRow("a")])),
        )
        queryMocks.useMasterDataCenterQuery.mockReturnValue({
            data: undefined,
            isPending: false,
        })

        renderState()
        expect(queryMocks.useMasterDataCenterQuery).toHaveBeenCalledWith(
            "unit-of-measures",
            "",
        )
    })

    it("resolves the preview row and loads its detail once selected", () => {
        const rows = [makeRow("a"), makeRow("b")]
        queryMocks.useMasterDataListQuery.mockReturnValue(
            listQueryState(listData(rows)),
        )
        queryMocks.useMasterDataCenterQuery.mockReturnValue({
            data: undefined,
            isPending: false,
        })

        const { result } = renderState({ enablePreview: true })
        expect(result.current.previewRow).toBeNull()

        act(() => {
            result.current.setPreviewId("b")
        })

        expect(queryMocks.useMasterDataCenterQuery).toHaveBeenLastCalledWith(
            "unit-of-measures",
            "b",
        )
        expect(result.current.previewRow?.stableId).toBe("b")
    })

    it("exports with the current filters and snapshot label", () => {
        navMocks.searchParams = new URLSearchParams("q=箱")
        const rows = [makeRow("a")]
        queryMocks.useMasterDataListQuery.mockReturnValue(
            listQueryState(listData(rows)),
        )
        queryMocks.useMasterDataCenterQuery.mockReturnValue({
            data: undefined,
            isPending: false,
        })
        exportMocks.handleExport.mockResolvedValue(undefined)

        const { result } = renderState()
        act(() => {
            result.current.onExport()
        })

        expect(exportMocks.handleExport).toHaveBeenCalledWith(
            {
                resource: "unit-of-measures",
                q: "箱",
                lifecycleStatus: "all",
                revisionTiming: "all",
            },
            expect.stringContaining("分类=计量单位"),
            "计量单位",
        )
    })

    it("skips the export when the list is empty", () => {
        queryMocks.useMasterDataListQuery.mockReturnValue(
            listQueryState(listData([])),
        )
        queryMocks.useMasterDataCenterQuery.mockReturnValue({
            data: undefined,
            isPending: false,
        })

        const { result } = renderState()
        act(() => {
            result.current.onExport()
        })

        expect(exportMocks.handleExport).not.toHaveBeenCalled()
    })

    it("derives the create permission from the account", () => {
        queryMocks.useMasterDataListQuery.mockReturnValue(
            listQueryState(listData([makeRow("a")])),
        )
        queryMocks.useMasterDataCenterQuery.mockReturnValue({
            data: undefined,
            isPending: false,
        })
        mockedAccountQuery.mockReturnValue({
            data: {
                userid: "u1",
                account: "acct",
                name: "张三",
                subject: "s1",
                role_ids: [],
                permissions: ["unit_of_measure:create"],
                account_kind: "staff",
            },
            isPending: false,
            isError: false,
            error: null,
        } as unknown as ReturnType<typeof useAccountProfileQuery>)

        const { result } = renderState()
        expect(result.current.canCreate).toBe(true)
    })

    it("builds a human-readable table description for active filters", () => {
        navMocks.searchParams = new URLSearchParams("lifecycleStatus=enabled")
        queryMocks.useMasterDataListQuery.mockReturnValue(
            listQueryState(listData([makeRow("a")])),
        )
        queryMocks.useMasterDataCenterQuery.mockReturnValue({
            data: undefined,
            isPending: false,
        })

        const { result } = renderState()
        expect(result.current.listTableDescription).toContain(
            "启用状态 当前启用",
        )
    })
})
