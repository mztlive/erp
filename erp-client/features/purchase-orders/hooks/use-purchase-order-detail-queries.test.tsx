import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook, waitFor } from "@testing-library/react"
import { QueryClientProvider } from "@tanstack/react-query"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

import {
    purchaseOrderKeys,
    useAcquireDraftTokenMutation,
    usePurchaseOrderCenterQuery,
    useReviewPurchaseOrderMutation,
    useSavePurchaseOrderDraftMutation,
    useStartPurchaseChangeMutation,
    useSubmitPurchaseOrderMutation,
} from "./queries"

const apiMocks = vi.hoisted(() => ({
    acquireDraftEditToken: vi.fn(),
    createPurchaseOrderFromBasis: vi.fn(),
    fetchCreationBases: vi.fn(),
    fetchPurchaseOrderCenter: vi.fn(),
    fetchPurchaseOrderExportData: vi.fn(),
    fetchPurchaseOrders: vi.fn(),
    reviewPurchaseOrder: vi.fn(),
    savePurchaseOrderDraft: vi.fn(),
    startPurchaseChange: vi.fn(),
    submitPurchaseOrderForReview: vi.fn(),
}))

vi.mock("@/features/purchase-orders/api/purchase-orders", () => ({
    acquireDraftEditToken: apiMocks.acquireDraftEditToken,
    createPurchaseOrderFromBasis: apiMocks.createPurchaseOrderFromBasis,
    fetchCreationBases: apiMocks.fetchCreationBases,
    fetchPurchaseOrderCenter: apiMocks.fetchPurchaseOrderCenter,
    fetchPurchaseOrderExportData: apiMocks.fetchPurchaseOrderExportData,
    fetchPurchaseOrders: apiMocks.fetchPurchaseOrders,
    reviewPurchaseOrder: apiMocks.reviewPurchaseOrder,
    savePurchaseOrderDraft: apiMocks.savePurchaseOrderDraft,
    startPurchaseChange: apiMocks.startPurchaseChange,
    submitPurchaseOrderForReview: apiMocks.submitPurchaseOrderForReview,
}))

beforeEach(() => {
    vi.clearAllMocks()
})

describe("purchaseOrderKeys", () => {
    it("keeps the detail key stable and layered under the domain root", () => {
        expect(purchaseOrderKeys.detail("po-1")).toEqual([
            "purchase-orders",
            "detail",
            "po-1",
        ])
        expect(purchaseOrderKeys.detail("po-1")[0]).toBe("purchase-orders")
    })
})

describe("usePurchaseOrderCenterQuery", () => {
    it("passes the id to the query function and surfaces data", async () => {
        apiMocks.fetchPurchaseOrderCenter.mockResolvedValue({
            purchaseOrderId: "po-1",
        })
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderCenterQuery("po-1"),
            { queryClient: createFreshQueryClient() },
        )
        expect(result.current.isPending).toBe(true)

        await waitFor(() =>
            expect(result.current.data).toEqual({ purchaseOrderId: "po-1" }),
        )
        expect(apiMocks.fetchPurchaseOrderCenter).toHaveBeenCalledWith("po-1")
        expect(result.current.isError).toBe(false)
    })

    it("is disabled for an empty id", () => {
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderCenterQuery(""),
            { queryClient: createFreshQueryClient() },
        )
        expect(apiMocks.fetchPurchaseOrderCenter).not.toHaveBeenCalled()
        expect(result.current.fetchStatus).toBe("idle")
    })

    it("surfaces query errors", async () => {
        apiMocks.fetchPurchaseOrderCenter.mockRejectedValue(
            new Error("加载失败"),
        )
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderCenterQuery("po-1"),
            { queryClient: createFreshQueryClient() },
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toEqual(new Error("加载失败"))
    })
})

describe("useAcquireDraftTokenMutation", () => {
    it("wires mutationFn to acquireDraftEditToken", async () => {
        apiMocks.acquireDraftEditToken.mockResolvedValue({
            draftEditToken: "tok-1",
            lockVersion: 3,
        })
        const { result } = renderHookWithProviders(
            () => useAcquireDraftTokenMutation(),
            { queryClient: createFreshQueryClient() },
        )
        await act(async () => {
            await result.current.mutateAsync("po-1")
        })
        expect(apiMocks.acquireDraftEditToken.mock.calls[0]?.[0]).toBe("po-1")
        await waitFor(() =>
            expect(result.current.data).toEqual({
                draftEditToken: "tok-1",
                lockVersion: 3,
            }),
        )
        expect(result.current.isPending).toBe(false)
        expect(result.current.isError).toBe(false)
    })
})

describe("useSavePurchaseOrderDraftMutation", () => {
    it("calls savePurchaseOrderDraft and invalidates the domain on success", async () => {
        apiMocks.savePurchaseOrderDraft.mockResolvedValue({
            status: "succeeded",
            data: { lockVersion: 4, draftContentHash: "h4", totals: {} },
            reference: "REF-1",
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useSavePurchaseOrderDraftMutation(),
            { queryClient: client },
        )
        const input = {
            purchaseOrderId: "po-1",
            expectedLockVersion: 3,
            draftEditToken: "tok-1",
            paymentTermCode: "POSTPAY_NET15",
            paymentTermLabel: "货到 15 天",
            lines: [],
            idempotencyKey: "k-1",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(apiMocks.savePurchaseOrderDraft.mock.calls[0]?.[0]).toEqual(
            input,
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: purchaseOrderKeys.all,
        })
        await waitFor(() =>
            expect(result.current.data?.status).toBe("succeeded"),
        )
    })

    it("does not invalidate when the save is not confirmed", async () => {
        apiMocks.savePurchaseOrderDraft.mockResolvedValue({
            status: "failed",
            message: "版本冲突",
            code: "CONFLICT",
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useSavePurchaseOrderDraftMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync({
                purchaseOrderId: "po-1",
                expectedLockVersion: 3,
                draftEditToken: "tok-1",
                paymentTermCode: "POSTPAY_NET15",
                paymentTermLabel: "货到 15 天",
                lines: [],
                idempotencyKey: "k-1",
            })
        })
        expect(invalidateSpy).not.toHaveBeenCalled()
        expect(result.current.isError).toBe(false)
    })
})

describe("useSubmitPurchaseOrderMutation", () => {
    it("calls submitPurchaseOrderForReview and invalidates on success", async () => {
        apiMocks.submitPurchaseOrderForReview.mockResolvedValue({
            status: "succeeded",
            data: {},
            reference: "REF-2",
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useSubmitPurchaseOrderMutation(),
            { queryClient: client },
        )
        const input = {
            purchaseOrderId: "po-1",
            expectedLockVersion: 4,
            expectedDraftContentHash: "h4",
            draftEditToken: "tok-1",
            idempotencyKey: "k-2",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(apiMocks.submitPurchaseOrderForReview.mock.calls[0]?.[0]).toEqual(
            input,
        )
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: purchaseOrderKeys.all,
        })
    })
})

describe("useReviewPurchaseOrderMutation", () => {
    it("calls reviewPurchaseOrder and invalidates on success", async () => {
        apiMocks.reviewPurchaseOrder.mockResolvedValue({
            status: "succeeded",
            data: {},
            reference: "REF-3",
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useReviewPurchaseOrderMutation(),
            { queryClient: client },
        )
        const input = {
            workItemId: "wi-1",
            expectedTaskVersion: "v1",
            expectedSubjectVersion: "v3",
            decision: {
                purchaseOrderId: "po-1",
                submissionId: "sub-1",
                expectedPurchaseOrderLockVersion: 3,
                reviewResult: "APPROVED" as const,
            },
            idempotencyKey: "k-3",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(apiMocks.reviewPurchaseOrder.mock.calls[0]?.[0]).toEqual(input)
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: purchaseOrderKeys.all,
        })
    })
})

describe("useStartPurchaseChangeMutation", () => {
    it("calls startPurchaseChange and invalidates on success", async () => {
        apiMocks.startPurchaseChange.mockResolvedValue({
            status: "succeeded",
            data: { changeId: "chg-1", baseRevisionNo: 2 },
            reference: "REF-4",
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useStartPurchaseChangeMutation(),
            { queryClient: client },
        )
        const input = {
            purchaseOrderId: "po-1",
            expectedLockVersion: 3,
            idempotencyKey: "k-4",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(apiMocks.startPurchaseChange.mock.calls[0]?.[0]).toEqual(input)
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: purchaseOrderKeys.all,
        })
    })
})

describe("query key identity", () => {
    it("reuses the cached result across renders with the same id", async () => {
        apiMocks.fetchPurchaseOrderCenter.mockResolvedValue({
            purchaseOrderId: "po-1",
        })
        const client = createFreshQueryClient()
        const wrapper = ({ children }: { children: React.ReactNode }) => (
            <QueryClientProvider client={client}>{children}</QueryClientProvider>
        )
        const { result, rerender } = renderHook(
            ({ id }: { id: string }) => usePurchaseOrderCenterQuery(id),
            {
                initialProps: { id: "po-1" },
                wrapper,
            },
        )
        await waitFor(() =>
            expect(result.current.data).toEqual({ purchaseOrderId: "po-1" }),
        )
        rerender({ id: "po-1" })
        expect(apiMocks.fetchPurchaseOrderCenter).toHaveBeenCalledTimes(1)
        expect(result.current.data).toEqual({ purchaseOrderId: "po-1" })
    })
})
