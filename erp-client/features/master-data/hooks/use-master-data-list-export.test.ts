import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useMasterDataListExport } from "./use-master-data-list-export"
import type { MasterDataListQuery } from "@/features/master-data/types"

const queryMocks = vi.hoisted(() => ({
    mutateAsync: vi.fn(),
    isPending: false,
}))

vi.mock("@/features/master-data/hooks/queries", () => ({
    useMasterDataExportMutation: () => ({
        mutateAsync: queryMocks.mutateAsync,
        isPending: queryMocks.isPending,
    }),
}))

const csvMocks = vi.hoisted(() => ({
    buildMasterDataExportCsv: vi.fn(),
    downloadCsv: vi.fn(),
}))

vi.mock("@/features/master-data/lib/export-csv", () => ({
    buildMasterDataExportCsv: csvMocks.buildMasterDataExportCsv,
    downloadCsv: csvMocks.downloadCsv,
}))

const query: MasterDataListQuery = {
    resource: "brands",
    q: "品牌A",
    lifecycleStatus: "enabled",
    revisionTiming: "all",
}

function refreshedRows(count: number) {
    return Array.from({ length: count }, (_, index) => ({
        objectType: "brands" as const,
        stableId: `b-${index}`,
        stableNo: `B-${index}`,
        name: `品牌 ${index}`,
        lifecycleStatus: "ENABLED" as const,
        lifecycleStatusLabel: "当前启用",
        lifecycleTone: "success" as const,
        revisionTiming: "CURRENT" as const,
        revisionTimingLabel: "当前生效",
        currentRevisionId: `r-${index}`,
        displayedRevisionId: `r-${index}`,
        revisionNo: 1,
        effectiveFrom: "2026-01-01",
        keyFacts: [],
        selectorEligibility: [],
        allowedActions: [],
        actionBlockers: [],
        lockVersion: 1,
        metricTags: [],
    }))
}

beforeEach(() => {
    queryMocks.mutateAsync.mockReset()
    queryMocks.isPending = false
    csvMocks.buildMasterDataExportCsv.mockReset()
    csvMocks.downloadCsv.mockReset()
})

describe("useMasterDataListExport", () => {
    it("re-queries with the current filter, builds and downloads the CSV", async () => {
        queryMocks.mutateAsync.mockResolvedValue({ rows: refreshedRows(3) })
        csvMocks.buildMasterDataExportCsv.mockReturnValue("csv,content")

        const { result } = renderHook(() => useMasterDataListExport())
        await act(async () => {
            await result.current.handleExport(query, "筛选=abc", "品牌")
        })

        expect(queryMocks.mutateAsync).toHaveBeenCalledWith(query)
        expect(csvMocks.buildMasterDataExportCsv).toHaveBeenCalledWith(
            refreshedRows(3),
            "筛选=abc",
        )
        expect(csvMocks.downloadCsv).toHaveBeenCalledWith(
            "csv,content",
            "基础资料-品牌",
        )
        expect(result.current.exportMeta).toMatchObject({
            rowCount: 3,
            filterSnapshotLabel: "筛选=abc",
        })
        expect(result.current.exportMeta?.jobId).toMatch(/^导出-\d{8}-/)
    })

    it("does not download when the refreshed result is empty", async () => {
        queryMocks.mutateAsync.mockResolvedValue({ rows: [] })

        const { result } = renderHook(() => useMasterDataListExport())
        await act(async () => {
            await result.current.handleExport(query, "筛选=abc", "品牌")
        })

        expect(csvMocks.buildMasterDataExportCsv).not.toHaveBeenCalled()
        expect(csvMocks.downloadCsv).not.toHaveBeenCalled()
        expect(result.current.exportMeta).toBeNull()
    })

    it("exposes the mutation state for pending UI", () => {
        queryMocks.isPending = true
        const { result } = renderHook(() => useMasterDataListExport())
        expect(result.current.exportMutation.isPending).toBe(true)
    })
})
