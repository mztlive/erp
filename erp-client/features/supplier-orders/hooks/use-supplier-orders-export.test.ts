import { act } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { renderHookWithProviders } from "@/features/test-utils"
import type { ExportJobResult } from "@/features/supplier-orders/types"
import { useSupplierOrdersExport } from "./use-supplier-orders-export"

const apiMocks = vi.hoisted(() => ({
    createSupplierOrderExportJob: vi.fn(),
}))

vi.mock("@/features/supplier-orders/api/index", () => apiMocks)

function makeJobResult(
    overrides: Partial<ExportJobResult> = {},
): ExportJobResult {
    return {
        jobId: "job_1",
        requestId: "req-1",
        rowCount: 3,
        permissionVersion: "server",
        fieldSetId: "w26-list-default-masked",
        maskDisclaimer: "导出使用系统筛选快照与字段权限打码。",
        expiresAt: "2026-08-21T00:00:00.000Z",
        downloadLabel: "供应商订单_job_1.csv",
        status: "queued",
        ...overrides,
    }
}

function renderExport() {
    return renderHookWithProviders(() => useSupplierOrdersExport())
}

beforeEach(() => {
    apiMocks.createSupplierOrderExportJob.mockReset()
})

describe("useSupplierOrdersExport — preview toggle", () => {
    it("opens and closes the preview", () => {
        const { result } = renderExport()
        expect(result.current.exportPreviewOpen).toBe(false)

        act(() => {
            result.current.openExportPreview()
        })
        expect(result.current.exportPreviewOpen).toBe(true)

        act(() => {
            result.current.closeExportPreview()
        })
        expect(result.current.exportPreviewOpen).toBe(false)
    })
})

describe("useSupplierOrdersExport — confirm", () => {
    it("submits the command built from the current list and clears the preview", async () => {
        apiMocks.createSupplierOrderExportJob.mockResolvedValue(makeJobResult())
        const { result } = renderExport()

        act(() => {
            result.current.openExportPreview()
        })
        await act(async () => {
            await result.current.confirmExport({
                total: 12,
                filterSummary: "全部 · 12 条",
            })
        })

        expect(apiMocks.createSupplierOrderExportJob).toHaveBeenCalledWith({
            selectionSnapshotId: expect.stringMatching(/^snap-req-w26-export-/),
            fieldSetId: "w26-list-default-masked",
            requestId: expect.stringMatching(/^req-w26-export-/),
            rowCount: 12,
            filterSummary: "全部 · 12 条",
        })
        expect(result.current.exportResult?.jobId).toBe("job_1")
        expect(result.current.exportPreviewOpen).toBe(false)
        expect(result.current.pendingExport).toBeNull()
    })

    it("keeps the pending command when the request fails", async () => {
        apiMocks.createSupplierOrderExportJob.mockRejectedValue(
            new Error("接口不可用"),
        )
        const { result } = renderExport()

        await act(async () => {
            await expect(
                result.current.confirmExport({
                    total: 12,
                    filterSummary: "全部",
                }),
            ).rejects.toThrow("接口不可用")
        })

        expect(result.current.pendingExport).not.toBeNull()
        expect(result.current.exportResult).toBeNull()
    })
})

describe("useSupplierOrdersExport — retry", () => {
    it("resubmits the original command snapshot after a failure", async () => {
        apiMocks.createSupplierOrderExportJob
            .mockRejectedValueOnce(new Error("接口不可用"))
            .mockResolvedValueOnce(makeJobResult({ jobId: "job_2" }))
        const { result } = renderExport()

        await act(async () => {
            await expect(
                result.current.confirmExport({
                    total: 5,
                    filterSummary: "全部",
                }),
            ).rejects.toThrow("接口不可用")
        })
        expect(result.current.pendingExport).not.toBeNull()

        await act(async () => {
            await result.current.retryExport()
        })

        expect(apiMocks.createSupplierOrderExportJob).toHaveBeenCalledTimes(2)
        const first = apiMocks.createSupplierOrderExportJob.mock.calls[0]![0]
        const second = apiMocks.createSupplierOrderExportJob.mock.calls[1]![0]
        expect(second.requestId).toBe(first.requestId)
        expect(result.current.exportResult?.jobId).toBe("job_2")
        expect(result.current.pendingExport).toBeNull()
        expect(result.current.exportPreviewOpen).toBe(false)
    })

    it("does nothing without a pending command", async () => {
        const { result } = renderExport()

        await act(async () => {
            await result.current.retryExport()
        })

        expect(apiMocks.createSupplierOrderExportJob).not.toHaveBeenCalled()
    })
})
