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
    submitPurchaseChange: vi.fn(),
    createPurchaseOrderFromBasis: vi.fn(),
}))

import {
    createPurchaseOrderFromBasis,
    fetchCreationBases,
    fetchPurchaseOrderExportData,
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

function makeListResult(
    overrides: Partial<{ page: number; rows: PurchaseOrderListItem[] }> = {},
) {
    return {
        rows: overrides.rows ?? [makeListItem()],
        total: overrides.rows?.length ?? 1,
        page: overrides.page ?? 1,
        pageSize: 20,
        metrics: [],
        freshness: {
            updatedAt: "2026-08-14T00:00:00.000Z",
            state: "fresh" as const,
        },
    }
}

function makeBasis(
    overrides: Partial<PurchaseCreationBasis> = {},
): PurchaseCreationBasis {
    return {
        basisId: "bas_1",
        salesOrderId: "so_1",
        salesOrderNo: "SO-1",
        customerName: "客户甲",
        salesOrderRevisionId: "sales-revision-1",
        supplierId: "sup_1",
        supplierName: "供应商A",
        purchaseType: "PHYSICAL",
        fulfillmentResponsibility: "WAREHOUSE",
        paymentTermCode: "POSTPAY_NET30",
        paymentTermLabel: "货到 30 天",
        lines: [
            {
                procurementConfirmationLineId: "pc-line-1",
                salesOrderLineId: "sales-line-1",
                salesOrderRevisionLineId: "sales-revision-line-1",
                itemName: "测试商品",
                salesQuantity: "10",
                coveredQuantity: "4",
                remainingQuantity: "6",
                maxCreateQuantity: "6",
                unit: "件",
                unitCostGross: "10",
                inputTaxRate: "0.13",
                expectedDeliveryDate: "2026-09-01",
                salesAllocationLabel: "销售明细 1",
            },
        ],
        estimatedGross: "100",
        consumed: false,
        ...overrides,
    }
}

function renderController() {
    const client = createFreshQueryClient()
    const rendered = renderHook(() => usePurchaseOrdersListController(), {
        wrapper: ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>
                {children}
            </QueryClientProvider>
        ),
    })
    return rendered
}

beforeEach(() => {
    vi.clearAllMocks()
    mockUseSearchParams.mockReturnValue(new URLSearchParams())
    mockedFetchList.mockResolvedValue(makeListResult())
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

    it("草稿变化不写 URL；应用筛选一次性写回并回第 1 页", async () => {
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())

        act(() => {
            result.current.filters.setSearchDraft("abc")
            result.current.filters.setStatusDraft("PENDING_REVIEW")
        })
        expect(mockRouter.replace).not.toHaveBeenCalled()

        act(() => {
            result.current.filters.applyFilters()
        })
        expect(mockRouter.replace).toHaveBeenCalledWith(
            "/procurement/orders?q=abc&status=PENDING_REVIEW",
            { scroll: false },
        )
        expect(result.current.filters.panelOpen).toBe(false)
    })

    it("应用筛选时清除同维度的指标粗筛", async () => {
        mockUseSearchParams.mockReturnValue(new URLSearchParams("metric=draft"))
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())
        expect(result.current.filters.appliedChips[0]?.label).toBe("指标：草稿")

        act(() => {
            result.current.filters.applyFilters()
        })
        expect(mockRouter.replace).toHaveBeenCalledWith("/procurement/orders", {
            scroll: false,
        })
    })

    it("清除筛选输出最小 URL 且保留排序与导航上下文", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams(
                "q=abc&status=DRAFT&metric=draft&sort=purchase_no:asc&page=3&basisId=bas_1",
            ),
        )
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())
        expect(result.current.filters.hasActiveFilters).toBe(true)

        act(() => {
            result.current.filters.clearAllFilters()
        })
        expect(mockRouter.replace).toHaveBeenCalledWith(
            "/procurement/orders?sort=purchase_no%3Aasc&basisId=bas_1",
            { scroll: false },
        )
    })

    it("重置更多条件只清主状态，保留关键词与指标", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("q=abc&status=DRAFT&metric=draft"),
        )
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())

        act(() => {
            result.current.filters.resetMoreFilters()
        })
        expect(mockRouter.replace).toHaveBeenCalledWith(
            "/procurement/orders?q=abc&metric=draft",
            { scroll: false },
        )
        expect(result.current.filters.panelOpen).toBe(true)
    })

    it("chip 单独移除单个条件", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("q=abc&status=DRAFT"),
        )
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())

        act(() => {
            result.current.filters.removeFilter("status")
        })
        expect(mockRouter.replace).toHaveBeenCalledWith(
            "/procurement/orders?q=abc",
            { scroll: false },
        )
    })

    it("有结构化状态的深链初始展开面板", async () => {
        mockUseSearchParams.mockReturnValue(new URLSearchParams("status=DRAFT"))
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())
        expect(result.current.filters.panelOpen).toBe(true)
    })

    it("已生效条件全部显性化为 chip", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("q=钢&status=PENDING_REVIEW&metric=fulfill"),
        )
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())
        expect(result.current.filters.appliedChips.map((c) => c.label)).toEqual(
            ["搜索：钢", "状态：审批中", "指标：待履约"],
        )
    })

    it("列表返回页码与 URL 不同步时校正 URL", async () => {
        mockUseSearchParams.mockReturnValue(new URLSearchParams("page=3"))
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

    it("进入列表时不预取创建依据", async () => {
        renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())
        expect(mockedFetchBases).not.toHaveBeenCalled()
    })

    it("建单成功后刷新依据，有剩余时保留弹框继续建单", async () => {
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
        act(() => {
            result.current.openCreateDialog()
        })
        await waitFor(() => expect(result.current.openBases).toHaveLength(1))

        act(() => {
            result.current.setSelectedBasisId("bas_1")
        })
        const lines = [{ salesOrderLineId: "sales-line-1", quantity: "6" }]
        await act(async () => {
            await result.current.handleCreate(lines)
        })
        expect(mockedCreate).toHaveBeenCalledTimes(1)
        expect(mockedCreate).toHaveBeenCalledWith(
            {
                basisId: "bas_1",
                purchaseType: "PHYSICAL",
                paymentTermCode: "POSTPAY_NET30",
                lines,
                idempotencyKey: expect.any(String),
            },
            expect.anything(),
        )
        expect(mockedFetchBases).toHaveBeenCalledTimes(2)
        expect(mockRouter.push).not.toHaveBeenCalled()
        expect(result.current.createOpen).toBe(true)
        expect(result.current.actionResult?.title).toBe("已创建采购草稿")
        expect(result.current.actionResult?.description).toContain(
            "仍有待采购数量，可继续建单",
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
        act(() => {
            result.current.openCreateDialog()
        })
        await waitFor(() => expect(result.current.openBases).toHaveLength(1))

        act(() => {
            result.current.setSelectedBasisId("bas_1")
        })
        expect(result.current.createOpen).toBe(true)
        const lines = [{ salesOrderLineId: "sales-line-1", quantity: "6" }]
        await act(async () => {
            await result.current.handleCreate(lines)
            await result.current.handleCreate(lines)
        })
        expect(mockedCreate).toHaveBeenCalledTimes(2)
        expect(mockedCreate.mock.calls[0]?.[0].idempotencyKey).toBe(
            mockedCreate.mock.calls[1]?.[0].idempotencyKey,
        )
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

    it("销售单关系筛选不误开建单框，显式 create 动作才打开", async () => {
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("salesOrderId=so_1"),
        )
        const relationOnly = renderController()
        await waitFor(() =>
            expect(mockedFetchList).toHaveBeenCalledWith(
                expect.objectContaining({ salesOrderId: "so_1" }),
            ),
        )
        expect(relationOnly.result.current.createOpen).toBe(false)
        relationOnly.unmount()

        mockedFetchBases.mockResolvedValue([
            makeBasis(),
            makeBasis({
                basisId: "bas_2",
                salesOrderId: "so_2",
                salesOrderNo: "SO-2",
            }),
        ])
        mockUseSearchParams.mockReturnValue(
            new URLSearchParams("salesOrderId=so_1&action=create"),
        )
        const createLink = renderController()
        await waitFor(() =>
            expect(createLink.result.current.createOpen).toBe(true),
        )
        await waitFor(() =>
            expect(createLink.result.current.openBases).toEqual([
                expect.objectContaining({ basisId: "bas_1" }),
            ]),
        )
    })

    it("筛选条件变化时焦点行重置为 0", async () => {
        mockUseSearchParams.mockReturnValue(new URLSearchParams())
        const { result, rerender } = renderController()
        await waitFor(() => expect(result.current.pageRows).toHaveLength(1))

        act(() => {
            result.current.setFocusedIndex(2)
        })
        expect(result.current.focusedIndex).toBe(2)

        mockUseSearchParams.mockReturnValue(new URLSearchParams("status=DRAFT"))
        rerender()
        await waitFor(() => expect(result.current.focusedIndex).toBe(0))
    })

    it("openDetail 跳转到采购单详情", async () => {
        const { result } = renderController()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalled())

        act(() => {
            result.current.openDetail("po_1")
        })
        expect(mockRouter.push).toHaveBeenCalledWith("/procurement/orders/po_1")
    })
})
