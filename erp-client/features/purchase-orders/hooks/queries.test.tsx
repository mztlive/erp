import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, waitFor, cleanup } from "@testing-library/react"
import type { ReactNode } from "react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"

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
    acquireDraftEditToken,
    createPurchaseOrderFromBasis,
    fetchCreationBases,
    fetchPurchaseOrderCenter,
    fetchPurchaseOrderExportData,
    fetchPurchaseOrders,
    reviewPurchaseOrder,
    savePurchaseOrderDraft,
    startPurchaseChange,
    submitPurchaseOrderForReview,
} from "@/features/purchase-orders/api/purchase-orders"
import type { PurchaseOrderListQuery } from "@/features/purchase-orders/api/purchase-orders"
import type {
    PurchaseCreationBasis,
    PurchaseOrderCenterView,
    PurchaseOrderListItem,
} from "@/features/purchase-orders/types"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import {
    purchaseOrderKeys,
    useAcquireDraftTokenMutation,
    useCreateFromBasisMutation,
    useCreationBasesQuery,
    usePurchaseOrderCenterQuery,
    usePurchaseOrderExportDataQuery,
    usePurchaseOrdersQuery,
    useReviewPurchaseOrderMutation,
    useSavePurchaseOrderDraftMutation,
    useStartPurchaseChangeMutation,
    useSubmitPurchaseOrderMutation,
} from "./queries"

const mockedFetchList = vi.mocked(fetchPurchaseOrders)
const mockedFetchExport = vi.mocked(fetchPurchaseOrderExportData)
const mockedFetchCenter = vi.mocked(fetchPurchaseOrderCenter)
const mockedFetchBases = vi.mocked(fetchCreationBases)
const mockedAcquire = vi.mocked(acquireDraftEditToken)
const mockedSave = vi.mocked(savePurchaseOrderDraft)
const mockedSubmit = vi.mocked(submitPurchaseOrderForReview)
const mockedReview = vi.mocked(reviewPurchaseOrder)
const mockedStartChange = vi.mocked(startPurchaseChange)
const mockedCreate = vi.mocked(createPurchaseOrderFromBasis)

const LIST_QUERY: PurchaseOrderListQuery = {
    q: "钢",
    status: "all",
    metric: "all",
    page: 1,
    pageSize: 20,
}

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

function makeListResult() {
    return {
        rows: [makeListItem()],
        total: 1,
        page: 1,
        pageSize: 20,
        metrics: [],
        freshness: {
            updatedAt: "2026-08-14T00:00:00.000Z",
            state: "fresh" as const,
        },
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

function makeCenter(): PurchaseOrderCenterView {
    return {
        identity: {
            purchaseOrderId: "po_1",
            purchaseNo: "PO-1",
            status: "DRAFT",
            statusLabel: "草稿",
            statusTone: "neutral",
            reviewStatus: "NONE",
            reviewLabel: "—",
            lockVersion: 1,
        },
        header: {
            salesOrderId: "so_1",
            salesOrderNo: "SO-1",
            supplierId: "sup_1",
            supplierSnapshot: "供应商A",
            purchaseType: "PHYSICAL",
            fulfillmentResponsibility: "WAREHOUSE",
            paymentTermCode: "POSTPAY_NET30",
            paymentTermLabel: "货到 30 天",
            ownerName: "—",
        },
        progress: {
            payment: "未付",
            invoice: "未收",
            fulfillment: "未开始",
            prepaymentGate: {
                state: "NOT_APPLICABLE",
                message: "",
                required: "0",
                allocated: "0",
                gap: "0",
                updatedAt: "2026-08-14T00:00:00.000Z",
            },
        },
        currentContent: {
            source: "DRAFT",
            version: 1,
            lines: [],
            totals: { gross: "0", net: "0", tax: "0" },
            costMasked: false,
        },
        allocations: [],
        fulfillmentSummary: {
            progressLabel: "未开始",
            progressTone: "neutral",
            inboundQty: "—",
            shippedQty: "—",
            remainingQty: "—",
        },
        changes: [],
        workflow: [],
        allowedActions: [],
        actionBlockers: [],
        fieldVisibility: {},
    }
}

function makeMutationWrapper(client: QueryClient) {
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    )
    return wrapper
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("purchaseOrderKeys", () => {
    it("按资源分层且可序列化稳定", () => {
        expect(purchaseOrderKeys.all).toEqual(["purchase-orders"])
        expect(purchaseOrderKeys.list(LIST_QUERY)).toEqual([
            "purchase-orders",
            "list",
            LIST_QUERY,
        ])
        expect(purchaseOrderKeys.detail("po_1")).toEqual([
            "purchase-orders",
            "detail",
            "po_1",
        ])
        expect(purchaseOrderKeys.bases()).toEqual([
            "purchase-orders",
            "creation-bases",
        ])
        expect(purchaseOrderKeys.exportData(LIST_QUERY)).toEqual([
            "purchase-orders",
            "export",
            LIST_QUERY,
        ])
    })
})

describe("usePurchaseOrdersQuery", () => {
    it("把查询输入传给 queryFn 并暴露数据", async () => {
        const result = makeListResult()
        mockedFetchList.mockResolvedValue(result)
        const client = createFreshQueryClient()
        const { result: hook } = renderHookWithProviders(
            () => usePurchaseOrdersQuery(LIST_QUERY),
            { queryClient: client },
        )
        expect(hook.current.isPending).toBe(true)

        await waitFor(() => expect(hook.current.isPending).toBe(false))
        expect(hook.current.data).toEqual(result)
        expect(mockedFetchList).toHaveBeenCalledWith(LIST_QUERY)
        expect(mockedFetchList).toHaveBeenCalledTimes(1)
    })

    it("相同输入保持单一缓存条目", async () => {
        mockedFetchList.mockResolvedValue(makeListResult())
        const client = createFreshQueryClient()
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrdersQuery(LIST_QUERY),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        rerender()
        const entries = client.getQueryCache().findAll()
        expect(entries).toHaveLength(1)
        expect(entries[0]?.queryKey).toEqual(purchaseOrderKeys.list(LIST_QUERY))
    })

    it("查询输入变化时重新请求", async () => {
        mockedFetchList.mockResolvedValue(makeListResult())
        const client = createFreshQueryClient()
        let query = LIST_QUERY
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrdersQuery(query),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        query = { ...LIST_QUERY, page: 2 }
        rerender()
        await waitFor(() => expect(mockedFetchList).toHaveBeenCalledWith(query))
        expect(mockedFetchList).toHaveBeenCalledTimes(2)
    })

    it("请求失败时无数据且 isError", async () => {
        mockedFetchList.mockRejectedValue(new Error("网络异常"))
        const { result } = renderHookWithProviders(() =>
            usePurchaseOrdersQuery(LIST_QUERY),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.data).toBeUndefined()
    })
})

describe("usePurchaseOrderExportDataQuery", () => {
    it("默认不请求，refetch 时拉取并返回行", async () => {
        const rows = [makeListItem()]
        mockedFetchExport.mockResolvedValue(rows)
        const { result } = renderHookWithProviders(() =>
            usePurchaseOrderExportDataQuery(LIST_QUERY),
        )
        await waitFor(() => expect(result.current.fetchStatus).toBe("idle"))
        expect(mockedFetchExport).not.toHaveBeenCalled()

        await act(async () => {
            await result.current.refetch()
        })
        expect(mockedFetchExport).toHaveBeenCalledWith(LIST_QUERY)
        await waitFor(() => expect(result.current.data).toEqual(rows))
    })
})

describe("usePurchaseOrderCenterQuery", () => {
    it("空 id 时禁用", () => {
        const { result } = renderHookWithProviders(() =>
            usePurchaseOrderCenterQuery(""),
        )
        expect(result.current.fetchStatus).toBe("idle")
        expect(mockedFetchCenter).not.toHaveBeenCalled()
    })

    it("按 id 取中心视图", async () => {
        const center = makeCenter()
        mockedFetchCenter.mockResolvedValue(center)
        const { result } = renderHookWithProviders(() =>
            usePurchaseOrderCenterQuery("po_1"),
        )
        await waitFor(() => expect(result.current.data).toEqual(center))
        expect(mockedFetchCenter).toHaveBeenCalledWith("po_1")
    })

    it("404 时返回 null 且不报错", async () => {
        mockedFetchCenter.mockResolvedValue(null)
        const { result } = renderHookWithProviders(() =>
            usePurchaseOrderCenterQuery("po_missing"),
        )
        await waitFor(() => expect(result.current.data).toBeNull())
        expect(result.current.isError).toBe(false)
    })
})

describe("useCreationBasesQuery", () => {
    it("拉取创建依据", async () => {
        const bases = [makeBasis()]
        mockedFetchBases.mockResolvedValue(bases)
        const { result } = renderHookWithProviders(() =>
            useCreationBasesQuery(),
        )
        await waitFor(() => expect(result.current.data).toEqual(bases))
        expect(mockedFetchBases).toHaveBeenCalledTimes(1)
    })

    it("enabled=false 时不请求", () => {
        const { result } = renderHookWithProviders(() =>
            useCreationBasesQuery({ enabled: false }),
        )
        expect(result.current.fetchStatus).toBe("idle")
        expect(mockedFetchBases).not.toHaveBeenCalled()
    })
})

describe("useAcquireDraftTokenMutation", () => {
    it("mutationFn 接 acquireDraftEditToken", async () => {
        mockedAcquire.mockResolvedValue({
            draftEditToken: "det:po_1:2",
            lockVersion: 2,
        })
        const { result } = renderHookWithProviders(() =>
            useAcquireDraftTokenMutation(),
        )
        const resolved = await act(async () =>
            result.current.mutateAsync("po_1"),
        )
        expect(mockedAcquire).toHaveBeenCalledWith("po_1", expect.anything())
        expect(resolved).toEqual({
            draftEditToken: "det:po_1:2",
            lockVersion: 2,
        })
    })
})

describe("useSavePurchaseOrderDraftMutation", () => {
    const input = {
        purchaseOrderId: "po_1",
        expectedLockVersion: 1,
        draftEditToken: "det:po_1:1",
        paymentTermCode: "POSTPAY_NET30",
        paymentTermLabel: "货到 30 天",
        lines: [],
        idempotencyKey: "save-1",
    }

    it("成功时失效全部采购单缓存", async () => {
        mockedSave.mockResolvedValue({
            status: "succeeded",
            data: {
                lockVersion: 2,
                draftContentHash: "SAVED-V2",
                totals: { gross: "0", net: "0", tax: "0" },
            },
            reference: "SAVED-V2",
        })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHook(
            () => useSavePurchaseOrderDraftMutation(),
            {
                wrapper: makeMutationWrapper(client),
            },
        )
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(mockedSave).toHaveBeenCalledWith(input, expect.anything())
        await waitFor(() => expect(invalidate).toHaveBeenCalled())
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: purchaseOrderKeys.all,
        })
    })

    it("失败结果不失效缓存", async () => {
        mockedSave.mockResolvedValue({
            status: "failed",
            message: "数据已更新",
            code: "CONFLICT",
        })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHook(
            () => useSavePurchaseOrderDraftMutation(),
            {
                wrapper: makeMutationWrapper(client),
            },
        )
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(invalidate).not.toHaveBeenCalled()
    })
})

describe("useSubmitPurchaseOrderMutation", () => {
    const input = {
        purchaseOrderId: "po_1",
        expectedLockVersion: 1,
        expectedDraftContentHash: "v1",
        draftEditToken: "det:po_1:1",
        idempotencyKey: "submit-1",
    }

    it("成功时失效全部采购单缓存", async () => {
        mockedSubmit.mockResolvedValue({
            status: "succeeded",
            data: {
                submissionId: "sub_1",
                submissionNo: "SUB-1",
                subjectHash: "sub_1",
                workItemId: "wi_1",
                taskVersion: "1",
                subjectVersion: "v1",
                purchaseNo: "PO-1",
                lockVersion: 2,
            },
            reference: "SUB-1",
        })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHook(() => useSubmitPurchaseOrderMutation(), {
            wrapper: makeMutationWrapper(client),
        })
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(mockedSubmit).toHaveBeenCalledWith(input, expect.anything())
        await waitFor(() => expect(invalidate).toHaveBeenCalled())
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: purchaseOrderKeys.all,
        })
    })

    it("unknown 结果不失效缓存", async () => {
        mockedSubmit.mockResolvedValue({
            status: "unknown",
            message: "处理结果待确认",
            idempotencyKey: "submit-1",
        })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHook(() => useSubmitPurchaseOrderMutation(), {
            wrapper: makeMutationWrapper(client),
        })
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(invalidate).not.toHaveBeenCalled()
    })
})

describe("useReviewPurchaseOrderMutation", () => {
    const input = {
        workItemId: "wi_1",
        expectedTaskVersion: "1",
        expectedSubjectVersion: "v1",
        decision: {
            purchaseOrderId: "po_1",
            submissionId: "sub_1",
            expectedPurchaseOrderLockVersion: 1,
            reviewResult: "APPROVED" as const,
            comment: "ok",
        },
        idempotencyKey: "review-1",
    }

    it("成功时失效全部采购单缓存", async () => {
        mockedReview.mockResolvedValue({
            status: "succeeded",
            data: {
                reviewResult: "APPROVED",
                lockVersion: 2,
                reference: "REVIEW-V2",
            },
            reference: "REVIEW-V2",
        })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHook(() => useReviewPurchaseOrderMutation(), {
            wrapper: makeMutationWrapper(client),
        })
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(mockedReview).toHaveBeenCalledWith(input, expect.anything())
        await waitFor(() => expect(invalidate).toHaveBeenCalled())
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: purchaseOrderKeys.all,
        })
    })
})

describe("useStartPurchaseChangeMutation", () => {
    const input = {
        purchaseOrderId: "po_1",
        expectedLockVersion: 1,
        idempotencyKey: "change-1",
    }

    it("成功时失效全部采购单缓存", async () => {
        mockedStartChange.mockResolvedValue({
            status: "succeeded",
            data: { changeId: "chg_1", baseRevisionNo: 1 },
            reference: "CHANGE-V1",
        })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHook(() => useStartPurchaseChangeMutation(), {
            wrapper: makeMutationWrapper(client),
        })
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(mockedStartChange).toHaveBeenCalledWith(input, expect.anything())
        await waitFor(() => expect(invalidate).toHaveBeenCalled())
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: purchaseOrderKeys.all,
        })
    })
})

describe("useCreateFromBasisMutation", () => {
    const input = { basisId: "bas_1", idempotencyKey: "create-1" }

    it("成功时失效全部采购单缓存", async () => {
        mockedCreate.mockResolvedValue({
            status: "succeeded",
            data: {
                purchaseOrderId: "po_new",
                draftLabel: "草稿 · PO-NEW",
                lockVersion: 1,
            },
            reference: "PO-NEW",
        })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHook(() => useCreateFromBasisMutation(), {
            wrapper: makeMutationWrapper(client),
        })
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(mockedCreate).toHaveBeenCalledWith(input, expect.anything())
        await waitFor(() => expect(invalidate).toHaveBeenCalled())
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: purchaseOrderKeys.all,
        })
    })

    it("failed 结果不失效缓存", async () => {
        mockedCreate.mockResolvedValue({
            status: "failed",
            message: "建单失败",
            code: "CONFLICT",
        })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHook(() => useCreateFromBasisMutation(), {
            wrapper: makeMutationWrapper(client),
        })
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(invalidate).not.toHaveBeenCalled()
    })
})
