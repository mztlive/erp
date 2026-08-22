import { act, cleanup, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import {
    createSalesOrderExportJob,
    fetchSalesOrders,
} from "@/features/sales-orders/api/sales-orders"
import type { SalesOrdersListQuery } from "@/features/sales-orders/api/contracts"
import { buildSalesOrdersListCsv } from "@/features/sales-orders/lib/sales-orders-list-csv"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import { useSalesOrdersListExport } from "./use-sales-orders-list-export"

vi.mock("@/features/sales-orders/api/sales-orders", () => ({
    fetchSalesOrders: vi.fn(),
    createSalesOrderExportJob: vi.fn(),
}))

const makeQuery = (): SalesOrdersListQuery => ({
    page: 2,
    pageSize: 20,
    summary: "all",
})

function makeSalesOrderListItem(
    overrides: Partial<SalesOrderListItem> = {},
): SalesOrderListItem {
    return {
        id: "so-1",
        documentNumber: "SO-2026-0001",
        customerName: "示例客户",
        contractId: "ct-1",
        contractNumber: "HT-2026-0312",
        contractCompanyName: "示例客户有限公司",
        contractRevisionLabel: "HT-2026-0312@v1",
        nature: "physical_service",
        originSystem: "erp",
        primaryStatus: { code: "effective", label: "已生效", tone: "success" },
        fulfillment: { label: "未开始", tone: "neutral" },
        collection: { label: "未收", tone: "neutral" },
        invoicing: { label: "未开", tone: "neutral" },
        amountGross: "1000.00",
        amountNet: "900.00",
        taxAmount: "100.00",
        receivedAmount: "0.00",
        invoicedAmount: "0.00",
        ownerName: "张三",
        submittedAt: "2026-01-01 10:00",
        welfareScene: "",
        version: 1,
        lockVersion: 1,
        currentRevisionNo: 1,
        settlementEntity: "主体",
        sellerEntity: "主体",
        paymentTerms: "月结",
        fulfillmentDeadline: "",
        lineItems: [],
        related: {
            purchaseOrders: 0,
            fulfillments: 0,
            receipts: 0,
            invoices: 0,
        },
        closeEligibility: {
            fulfillmentComplete: false,
            receivableSettled: false,
            invoiceComplete: false,
            eligibleToClose: false,
            blockers: [],
            note: "",
        },
        natureLocked: true,
        commercialReadOnly: false,
        revisions: [],
        procurementRejection: null,
        activeLowMarginManagerConfirmation: null,
        activeChangeOrder: null,
        allowedActions: [],
        actionBlockers: [],
        ...overrides,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {})
    vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:fake")
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {})
})

afterEach(() => {
    cleanup()
})

describe("useSalesOrdersListExport", () => {
    it("总数为 0 时不创建任务也不下载", async () => {
        const { result } = renderHookWithProviders(
            () => useSalesOrdersListExport(makeQuery(), 0),
            { queryClient: createFreshQueryClient() },
        )

        await act(async () => {
            await result.current.exportCsv()
        })

        expect(createSalesOrderExportJob).not.toHaveBeenCalled()
        expect(fetchSalesOrders).not.toHaveBeenCalled()
        expect(result.current.exportJob).toBeNull()
    })

    it("导出成功时创建任务、拉取全集并触发 CSV 下载", async () => {
        vi.mocked(createSalesOrderExportJob).mockResolvedValue({
            jobId: "job-1",
            status: "queued",
            rowCount: 2,
            permissionVersion: "pv-1",
            createdAt: "2026-01-01T00:00:00Z",
            downloadLabel: "销售单导出_EXP-1",
        })
        vi.mocked(fetchSalesOrders).mockResolvedValue({
            items: [
                makeSalesOrderListItem({ id: "so-1" }),
                makeSalesOrderListItem({
                    id: "so-2",
                    documentNumber: "SO-2026-0002",
                    nature: "card_voucher",
                    originSystem: "mall",
                }),
            ],
            total: 2,
            page: 1,
            pageSize: 2,
            queriedAt: "2026-01-01T00:00:00Z",
        })
        const query = makeQuery()
        const { result } = renderHookWithProviders(
            () => useSalesOrdersListExport(query, 2),
            { queryClient: createFreshQueryClient() },
        )

        await act(async () => {
            await result.current.exportCsv()
        })

        expect(createSalesOrderExportJob).toHaveBeenCalledTimes(1)
        expect(vi.mocked(createSalesOrderExportJob).mock.calls[0]?.[0]).toEqual(
            { rowCount: 2 },
        )
        expect(fetchSalesOrders).toHaveBeenCalledWith({
            ...query,
            page: 1,
            pageSize: 2,
        })
        expect(result.current.exportJob).toMatchObject({
            jobId: "job-1",
            rowCount: 2,
            fileName: expect.stringMatching(/^销售单列表_\d{8}_\d{4}\.csv$/),
        })
        expect(HTMLAnchorElement.prototype.click).toHaveBeenCalledTimes(1)

        const blob = vi.mocked(URL.createObjectURL).mock.calls[0]?.[0]
        expect(blob).toBeInstanceOf(Blob)
        const text = await (blob as Blob).text()
        // Blob.text() 按 UTF-8 解码会剥掉 BOM，文件内容本身以 BOM 开头（见纯函数断言）。
        expect(text.startsWith("# 导出时间")).toBe(true)
        expect(text).toContain(
            "销售单号,客户,合同,业务性质,状态,创建来源,成交金额（含税）,负责人,提交时间",
        )
        expect(text).toContain(
            '"SO-2026-0002","示例客户","HT-2026-0312","卡券","已生效","创建于商城","1000.00","张三","2026-01-01 10:00"',
        )
    })

    it("任务创建失败时不写入导出结果并向上抛错", async () => {
        vi.mocked(createSalesOrderExportJob).mockRejectedValue(
            new Error("boom"),
        )
        const { result } = renderHookWithProviders(
            () => useSalesOrdersListExport(makeQuery(), 5),
            { queryClient: createFreshQueryClient() },
        )

        await act(async () => {
            await expect(result.current.exportCsv()).rejects.toThrow("boom")
        })

        expect(fetchSalesOrders).not.toHaveBeenCalled()
        expect(result.current.exportJob).toBeNull()
    })

    it("isExporting 在任务进行中为 true", async () => {
        vi.mocked(createSalesOrderExportJob).mockImplementation(
            () => new Promise(() => {}),
        )
        const { result } = renderHookWithProviders(
            () => useSalesOrdersListExport(makeQuery(), 5),
            { queryClient: createFreshQueryClient() },
        )

        expect(result.current.isExporting).toBe(false)
        act(() => {
            void result.current.exportCsv()
        })
        await waitFor(() => expect(result.current.isExporting).toBe(true))
    })
})

describe("buildSalesOrdersListCsv", () => {
    it("生成带 BOM 的 CSV 并转义引号", () => {
        const now = new Date(2026, 7, 14, 5, 6, 7)
        const { fileName, content } = buildSalesOrdersListCsv(
            [
                makeSalesOrderListItem({
                    documentNumber: 'SO-"2026',
                }),
            ],
            now,
        )

        expect(fileName).toBe("销售单列表_20260814_0506.csv")
        expect(content).toContain("\uFEFF# 导出时间 ")
        expect(content).toContain('"SO-""2026"')
    })

    it("空列表仍包含表头与说明行", () => {
        const { content } = buildSalesOrdersListCsv([], new Date(2026, 0, 1))
        const lines = content.replace("\uFEFF", "").split("\n")
        expect(lines).toHaveLength(2)
        expect(lines[1]).toBe(
            "销售单号,客户,合同,业务性质,状态,创建来源,成交金额（含税）,负责人,提交时间",
        )
    })
})
