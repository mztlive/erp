import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, waitFor, cleanup } from "@testing-library/react"
import { QueryClientProvider } from "@tanstack/react-query"
import type { ReactNode } from "react"

const mockRouter = { push: vi.fn(), replace: vi.fn(), back: vi.fn() }
const mockUseSearchParams = vi.fn(() => new URLSearchParams())

vi.mock("next/navigation", () => ({
    useRouter: () => mockRouter,
    useSearchParams: () => mockUseSearchParams(),
    usePathname: () => "/procurement/orders",
    useParams: () => ({}),
}))

vi.mock("@/features/purchase-orders/api/purchase-orders", () => ({
    fetchPurchaseOrders: vi.fn(),
    fetchPurchaseOrderExportData: vi.fn(),
    fetchPurchaseOrderCenter: vi.fn(),
    fetchCreationBases: vi.fn(),
    acquireDraftEditToken: vi.fn(),
    savePurchaseOrderDraft: vi.fn(),
    submitPurchaseOrderForReview: vi.fn(),
    reviewPurchaseOrder: vi.fn(),
    startPurchaseChange: vi.fn(),
    createPurchaseOrderFromBasis: vi.fn(),
}))

import {
    createPurchaseOrderFromBasis,
    fetchCreationBases,
    fetchPurchaseOrderExportData,
    fetchPurchaseOrderCenter,
    fetchPurchaseOrders,
} from "@/features/purchase-orders/api/purchase-orders"
import type {
    PurchaseCreationBasis,
    PurchaseOrderListItem,
} from "@/features/purchase-orders/types"
import { createFreshQueryClient } from "@/features/test-utils"
import { usePurchaseOrdersListController } from "./use-purchase-orders-list-controller"

const mockedFetchList = vi.mocked(fetchPurchaseOrders)
const mockedFetchExport = vi.mocked(fetchPurchaseOrderExportData)
const mockedFetchCenter = vi.mocked(fetchPurchaseOrderCenter)
const mockedFetchBases = vi.mocked(fetchCreationBases)
const mockedCreate = vi.mocked(createPurchaseOrderFromBasis)

function makeListItem(): PurchaseOrderListItem {
    return {
        purchaseOrderId: "po_1",
        purchaseNo: "PO-1",
        status: "DRAFT",
        statusLabel: "草稿",
        statusTone: "neutral",
        reviewStatus: "NONE",
        reviewLabel: "—",
        salesOrderId: "so_1",
        salesOrderNo: "SO-1",
        supplierId: "sup_1",
        supplierName: "供应商A",
        purchaseType: "PHYSICAL",
        fulfillmentResponsibility: "WAREHOUSE",
        paymentTermCode: "",
        paymentTermLabel: "—",
        ownerName: "—",
        grossAmount: "10",
        netAmount: "9",
        taxAmount: "1",
        costMasked: false,
        paymentProgress: "未付",
        invoiceProgress: "未收",
        fulfillmentProgress: "未开始",
        paymentGate: "NOT_APPLICABLE",
        updatedAt: "2026-08-14T00:00:00.000Z",
        allowedActions: [],
        actionBlockers: [],
    }
}

function makeListResult(overrides: Partial<{ page: number; rows: PurchaseOrderListItem[] }> = {}) {
    return {
        rows: overrides.rows ?? [makeListItem()],
        total: overrides.rows?.length ?? 1,
        page: overrides.page ?? 1,
        pageSize: 20,
        metrics: [],
        freshness: { updatedAt: "2026-08-14T00:00:00.000Z", state: "fresh" as const },
    }
}

function makeBasis(): PurchaseCreationBasis {
    return {
        basisId: "bas_1",
        salesOrderId: "so_1",
        salesOrderNo: "SO-1",
        salesSubmissionId: "sub_1",
        salesSubmissionNo: 0,
        supplierId: "sup_1",
        supplierName: "供应商A",
        purchaseType: "PHYSICAL",
        fulfillmentResponsibility: "WAREHOUSE",
        paymentTermCode: "POSTPAY_NET30",
        paymentTermLabel: "货到 30 天",
        lines: [],
        estimatedGross: "100",
        consumed: false,
    }
}

function renderController() {
    const client = createFreshQueryClient()
    const rendered = renderHook(() => usePurchaseOrdersListController(), {
        wrapper: ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>{children}</QueryClientProvider>
        ),
    })
    return rendered
}

beforeEach(() => {
    vi.clearAllMocks()
    mockUseSearchParams.mockReturnValue(new URLSearchParams())
    mockedFetchList.mockResolvedValue(makeListResult())
    mockedFetchCenter.mockResolvedValue(null)
    mockedFetchBases.mockResolvedValue([])
    mockedFetchExport.mockResolvedValue([])
    URL.createObjectURL = vi.fn(() => "blob:mock")
    URL.revokeObjectURL = vi.fn()
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {})
})

afterEach(() => {
    cleanup()
})

describe("usePurchaseOrdersListController", () => {
    it("把 URL 派生的查询输入传给列表查询", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("status=DRAFT&page=2"),
        )
        renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())
        expect(mockedFetchList).toHaveBeenCalledWith({
            q: undefined,
            status: "DRAFT",
            metric: "all",
            page: 2,
            pageSize: 20,
            sortBy: undefined,
            sortDir: undefined,
        })
    })

    it("metric=pending_create 时列表查询按全部处理", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("metric=pending_create"),
        )
        renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())
        expect(mockedFetchList).toHaveBeenCalledWith(
            expect.objectContaining({ metric: "all" }),
        )
    })

    it("搜索防抖：300ms 后把搜索词写回 URL 并回到第 1 页", async () => {
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())

        act(() => {
            result.current.setSearchDraft("abc")
        })
        expect(mockRouter.replace).not.toHaveBeenCalled()
        await waitFor(() =>
            expect(mockRouter.replace).toHaveBeenCalledWith(
                "/procurement/orders?q=abc",
                { scroll: false },
            ),
        )
    })

    it("搜索词与 URL 一致时不重复写回", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("q=abc"),
        )
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())

        act(() => {
            result.current.setSearchDraft("abc")
        })
        await new Promise((resolve) => setTimeout(resolve, 400))
        expect(mockRouter.replace).not.toHaveBeenCalled()
    })

    it("清除筛选输出最小 URL", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("q=abc&status=DRAFT"),
        )
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())
        expect(result.current.hasActiveFilters).toBe(true)

        act(() => {
            result.current.clearFilters()
        })
        expect(mockRouter.replace).toHaveBeenCalledWith(
            "/procurement/orders",
            { scroll: false },
        )
    })

    it("列表返回页码与 URL 不同步时校正 URL", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("page=3"),
        )
        mockedFetchList.mockResolvedValue(makeListResult({ page: 5 }))
        renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())
        await waitFor(() =>
            expect(mockRouter.replace).toHaveBeenCalledWith(
                "/procurement/orders?page=5",
                { scroll: false },
            ),
        )
    })

    it("导出成功：生成 CSV、触发下载并记录结果", async () => {
        mockedFetchExport.mockResolvedValue([makeListItem(), makeListItem()])
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())

        await act(async () => {
            await result.current.exportCsv()
        })
        expect(mockedFetchExport).toHaveBeenCalledTimes(1)
        expect(URL.createObjectURL).toHaveBeenCalledTimes(1)
        const blob = vi.mocked(URL.createObjectURL).mock.calls[0][0] as Blob
        const text = await blob.text()
        expect(text).toContain(
            "采购单号,状态,供应商,来源销售单,类型,含税金额,付款,履约,负责人",
        )
        expect(text).toContain('"PO-1","草稿"')
        expect(HTMLAnchorElement.prototype.click).toHaveBeenCalledTimes(1)
        expect(result.current.actionResult).toEqual({
            status: "succeeded",
            title: "导出已生成",
            description: "已下载当前筛选 2 条。",
            reference: "EXPORT-2",
        })
    })

    it("导出无数据时不触发下载", async () => {
        mockedFetchExport.mockResolvedValue([])
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())

        await act(async () => {
            await result.current.exportCsv()
        })
        expect(URL.createObjectURL).not.toHaveBeenCalled()
        expect(result.current.actionResult).toBeNull()
    })

    it("建单成功：调用 API、关闭弹框并跳转编辑页", async () => {
        mockedFetchBases.mockResolvedValue([makeBasis()])
        mockedCreate.mockResolvedValue({
            status: "succeeded",
            data: {
                purchaseOrderId: "po_new",
                draftLabel: "草稿 · PO-NEW",
                lockVersion: 1,
            },
            reference: "PO-NEW",
        })
        const { result } = renderController()
        await waitFor(() => expect(result.current.openBases).toHaveLength(1))

        act(() => {
            result.current.setSelectedBasisId("bas_1")
        })
        await act(async () => {
            await result.current.handleCreate()
        })
        expect(mockedCreate).toHaveBeenCalledTimes(1)
        expect(mockedCreate.mock.calls[0][0].basisId).toBe("bas_1")
        expect(mockedCreate.mock.calls[0][0].idempotencyKey).toMatch(
            /^create-basis-bas_1-\d+$/,
        )
        expect(mockRouter.push).toHaveBeenCalledWith(
            "/procurement/orders/po_new?mode=edit",
        )
        expect(result.current.createOpen).toBe(false)
        expect(result.current.actionResult?.title).toBe("已创建采购草稿")
        expect(result.current.actionResult?.description).toContain(
            "草稿 · PO-NEW",
        )
    })

    it("建单失败：记录失败结果且不跳转", async () => {
        mockedFetchBases.mockResolvedValue([makeBasis()])
        mockedCreate.mockResolvedValue({
            status: "failed",
            message: "依据已不可用",
            code: "CONFLICT",
        })
        const { result } = renderController()
        await waitFor(() => expect(result.current.openBases).toHaveLength(1))

        act(() => {
            result.current.openCreateDialog()
            result.current.setSelectedBasisId("bas_1")
        })
        expect(result.current.createOpen).toBe(true)
        await act(async () => {
            await result.current.handleCreate()
        })
        expect(mockRouter.push).not.toHaveBeenCalled()
        expect(result.current.createOpen).toBe(true)
        expect(result.current.actionResult).toEqual({
            status: "failed",
            title: "建单失败",
            description: "依据已不可用",
        })
    })

    it("URL 携带 basisId 时自动打开建单弹框并选中依据", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("basisId=bas_9"),
        )
        const { result } = renderController()
        await waitFor(() => expect(result.current.createOpen).toBe(true))
        expect(result.current.selectedBasisId).toBe("bas_9")
    })

    it("筛选条件变化时焦点行重置为 0", async () => {
        mockUseSearchParams.mockReturnValue(new URLSearchParams())
        const { result, rerender } = renderController()
        await waitFor(() => expect(result.current.pageRows).toHaveLength(1))

        act(() => {
            result.current.setFocusedIndex(2)
        })
        expect(result.current.focusedIndex).toBe(2)

        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("status=DRAFT"),
        )
        rerender()
        await waitFor(() => expect(result.current.focusedIndex).toBe(0))
    })

    it("预览按 id 取中心视图", async () => {
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())
        expect(mockedFetchCenter).not.toHaveBeenCalled()

        act(() => {
            result.current.setPreviewId("po_1")
        })
        await waitFor(() => expect(mockedFetchCenter).toHaveBeenCalledWith("po_1"))
    })
})
