import { act, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import type { SupplierOrderListQuery } from "@/features/supplier-orders/types"

const apiMocks = vi.hoisted(() => ({
    addCollaborationNote: vi.fn(),
    completeSupplierOrderTask: vi.fn(),
    createSupplierOrderExportJob: vi.fn(),
    fetchSupplierOrderDetail: vi.fn(),
    fetchSupplierOrders: vi.fn(),
    querySupplierResult: vi.fn(),
    replaySupplierOrder: vi.fn(),
    revealSupplierOrderAddress: vi.fn(),
    submitAfterSalesAction: vi.fn(),
}))

vi.mock("@/features/supplier-orders/api/index", () => apiMocks)

import {
    useAddNoteMutation,
    useAfterSalesActionMutation,
    useCompleteOrderTaskMutation,
    useQueryResultMutation,
    useReplayOrderMutation,
    useRevealAddressMutation,
    useSupplierOrderDetailQuery,
    useSupplierOrderExportMutation,
    useSupplierOrdersQuery,
} from "./queries"

const listQuery: SupplierOrderListQuery = {
    view: "actionable",
    page: 2,
    pageSize: 20,
    q: "A-1",
    sortBy: "lastBusinessAt",
    sortDir: "desc",
}

beforeEach(() => {
    for (const mock of Object.values(apiMocks)) mock.mockReset()
})

function renderWithClient<Result>(callback: () => Result) {
    const client = createFreshQueryClient()
    const invalidate = vi.spyOn(client, "invalidateQueries")
    const rendered = renderHookWithProviders<Result, unknown>(callback, {
        queryClient: client,
    })
    return { ...rendered, client, invalidate }
}

describe("useSupplierOrdersQuery", () => {
    it("uses the stable supplier-orders list key and passes the query to the api", async () => {
        apiMocks.fetchSupplierOrders.mockResolvedValue({ rows: [] })
        const { result, client } = renderWithClient(() =>
            useSupplierOrdersQuery(listQuery),
        )

        await waitFor(() => expect(result.current.isPending).toBe(false))

        const cacheKey = client.getQueryCache().getAll()[0]!.queryKey
        expect(cacheKey).toEqual(["supplier-orders", "list", listQuery])
        expect(apiMocks.fetchSupplierOrders).toHaveBeenCalledWith(listQuery)
        expect(result.current.data?.rows).toEqual([])
    })

    it("does not refetch when the query object is structurally equal", async () => {
        apiMocks.fetchSupplierOrders.mockResolvedValue({ rows: [] })
        let q: SupplierOrderListQuery = listQuery
        const { result, rerender } = renderWithClient(() =>
            useSupplierOrdersQuery(q),
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))
        expect(apiMocks.fetchSupplierOrders).toHaveBeenCalledTimes(1)

        // 新引用但结构相等 → queryKey hash 不变 → 不重新请求
        q = { ...listQuery }
        rerender()
        expect(apiMocks.fetchSupplierOrders).toHaveBeenCalledTimes(1)
    })

    it("exposes the error state when the request fails", async () => {
        apiMocks.fetchSupplierOrders.mockRejectedValue(new Error("接口不可用"))
        const { result } = renderWithClient(() =>
            useSupplierOrdersQuery(listQuery),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useSupplierOrderDetailQuery", () => {
    it("keys on orderId plus optional workItemId", async () => {
        apiMocks.fetchSupplierOrderDetail.mockResolvedValue({})
        const { result, client } = renderWithClient(() =>
            useSupplierOrderDetailQuery({
                orderId: "so_1",
                workItemId: "wi_1",
            }),
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        const cacheKey = client.getQueryCache().getAll()[0]!.queryKey
        expect(cacheKey).toEqual(["supplier-orders", "detail", "so_1", "wi_1"])
        expect(apiMocks.fetchSupplierOrderDetail).toHaveBeenCalledWith({
            orderId: "so_1",
            workItemId: "wi_1",
        })
    })

    it("normalizes a missing workItemId to null in the key", async () => {
        apiMocks.fetchSupplierOrderDetail.mockResolvedValue({})
        const { result, client } = renderWithClient(() =>
            useSupplierOrderDetailQuery({ orderId: "so_1" }),
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        const cacheKey = client.getQueryCache().getAll()[0]!.queryKey
        expect(cacheKey).toEqual(["supplier-orders", "detail", "so_1", null])
    })

    it("stays disabled without an orderId", () => {
        apiMocks.fetchSupplierOrderDetail.mockResolvedValue({})
        const { result } = renderWithClient(() =>
            useSupplierOrderDetailQuery({ orderId: "", enabled: true }),
        )
        expect(result.current.fetchStatus).toBe("idle")
        expect(apiMocks.fetchSupplierOrderDetail).not.toHaveBeenCalled()
    })

    it("stays disabled with enabled=false even for a valid orderId", () => {
        apiMocks.fetchSupplierOrderDetail.mockResolvedValue({})
        const { result } = renderWithClient(() =>
            useSupplierOrderDetailQuery({ orderId: "so_1", enabled: false }),
        )
        expect(result.current.fetchStatus).toBe("idle")
        expect(apiMocks.fetchSupplierOrderDetail).not.toHaveBeenCalled()
    })
})

async function mutateSettled<T>(
    result: { current: { mutateAsync: (input: T) => Promise<unknown> } },
    input: T,
) {
    await act(async () => {
        await result.current.mutateAsync(input)
    })
}

/** TanStack v5 调用 mutationFn(variables, options)，断言只看业务入参 */
function firstArgOf(mock: ReturnType<typeof vi.fn>): unknown {
    return mock.mock.calls[0]![0]
}

describe("useQueryResultMutation", () => {
    it("submits through querySupplierResult and invalidates orders on success", async () => {
        apiMocks.querySupplierResult.mockResolvedValue({ status: "succeeded" })
        const { result, invalidate } = renderWithClient(() =>
            useQueryResultMutation(),
        )
        const input = { commandKind: "OBJECT", orderId: "so_1" } as never

        await mutateSettled(result, input)

        expect(firstArgOf(apiMocks.querySupplierResult)).toEqual(input)
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["supplier-orders"],
        })
    })

    it("also invalidates on unknown results", async () => {
        apiMocks.querySupplierResult.mockResolvedValue({ status: "unknown" })
        const { result, invalidate } = renderWithClient(() =>
            useQueryResultMutation(),
        )
        await mutateSettled(result, {} as never)
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["supplier-orders"],
        })
    })

    it("does not invalidate on blocked results", async () => {
        apiMocks.querySupplierResult.mockResolvedValue({ status: "blocked" })
        const { result, invalidate } = renderWithClient(() =>
            useQueryResultMutation(),
        )
        await mutateSettled(result, {} as never)
        expect(invalidate).not.toHaveBeenCalled()
    })

    it("propagates api errors", async () => {
        apiMocks.querySupplierResult.mockRejectedValue(new Error("查询失败"))
        const { result } = renderWithClient(() => useQueryResultMutation())
        await expect(
            act(async () => {
                await result.current.mutateAsync({} as never)
            }),
        ).rejects.toThrow("查询失败")
    })
})

describe("useReplayOrderMutation", () => {
    it("wires replaySupplierOrder and invalidates on success", async () => {
        apiMocks.replaySupplierOrder.mockResolvedValue({ status: "succeeded" })
        const { result, invalidate } = renderWithClient(() =>
            useReplayOrderMutation(),
        )
        const input = { commandKind: "OBJECT", action: "REPLAY" } as never

        await mutateSettled(result, input)

        expect(firstArgOf(apiMocks.replaySupplierOrder)).toEqual(input)
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["supplier-orders"],
        })
    })

    it("skips invalidation for non-succeeded results", async () => {
        apiMocks.replaySupplierOrder.mockResolvedValue({ status: "blocked" })
        const { result, invalidate } = renderWithClient(() =>
            useReplayOrderMutation(),
        )
        await mutateSettled(result, {} as never)
        expect(invalidate).not.toHaveBeenCalled()
    })
})

describe("useCompleteOrderTaskMutation", () => {
    it("wires completeSupplierOrderTask and invalidates on success", async () => {
        apiMocks.completeSupplierOrderTask.mockResolvedValue({
            status: "succeeded",
        })
        const { result, invalidate } = renderWithClient(() =>
            useCompleteOrderTaskMutation(),
        )
        const input = { workItemId: "wi_1" } as never

        await mutateSettled(result, input)

        expect(firstArgOf(apiMocks.completeSupplierOrderTask)).toEqual(input)
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["supplier-orders"],
        })
    })
})

describe("useAfterSalesActionMutation", () => {
    it("wires submitAfterSalesAction and invalidates on success", async () => {
        apiMocks.submitAfterSalesAction.mockResolvedValue({
            status: "succeeded",
        })
        const { result, invalidate } = renderWithClient(() =>
            useAfterSalesActionMutation(),
        )
        const input = { orderId: "so_1", action: "CANCEL" } as never

        await mutateSettled(result, input)

        expect(firstArgOf(apiMocks.submitAfterSalesAction)).toEqual(input)
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["supplier-orders"],
        })
    })
})

describe("useRevealAddressMutation", () => {
    it("wires revealSupplierOrderAddress and invalidates on success", async () => {
        apiMocks.revealSupplierOrderAddress.mockResolvedValue({
            status: "succeeded",
        })
        const { result, invalidate } = renderWithClient(() =>
            useRevealAddressMutation(),
        )
        const input = { orderId: "so_1" } as never

        await mutateSettled(result, input)

        expect(firstArgOf(apiMocks.revealSupplierOrderAddress)).toEqual(input)
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["supplier-orders"],
        })
    })
})

describe("useAddNoteMutation", () => {
    it("wires addCollaborationNote and invalidates on success", async () => {
        apiMocks.addCollaborationNote.mockResolvedValue({ status: "succeeded" })
        const { result, invalidate } = renderWithClient(() =>
            useAddNoteMutation(),
        )
        const input = { orderId: "so_1", comment: "备注" } as never

        await mutateSettled(result, input)

        expect(firstArgOf(apiMocks.addCollaborationNote)).toEqual(input)
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["supplier-orders"],
        })
    })
})

describe("useSupplierOrderExportMutation", () => {
    it("submits the export command and invalidates on success", async () => {
        apiMocks.createSupplierOrderExportJob.mockResolvedValue({
            status: "succeeded",
            jobId: "job_1",
        })
        const { result, invalidate } = renderWithClient(() =>
            useSupplierOrderExportMutation(),
        )
        const command = {
            selectionSnapshotId: "snap-1",
            fieldSetId: "w26-list-default-masked",
            requestId: "req-1",
            rowCount: 3,
            filterSummary: "全部 · 3 条",
        }

        await mutateSettled(result, command)

        expect(firstArgOf(apiMocks.createSupplierOrderExportJob)).toEqual(
            command,
        )
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: ["supplier-orders"],
        })
    })
})
