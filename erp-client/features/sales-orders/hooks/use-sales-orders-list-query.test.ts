import { cleanup, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { fetchSalesOrders } from "@/features/sales-orders/api/sales-orders"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { parseSalesOrdersSearchParams } from "@/features/sales-orders/lib/url-state"
import { useSalesOrdersListQuery } from "./use-sales-orders-list-query"

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: vi.fn(),
}))

vi.mock("@/features/sales-orders/api/sales-orders", () => ({
    fetchSalesOrders: vi.fn(),
}))

const makeUrl = (raw: string) =>
    parseSalesOrdersSearchParams(new URLSearchParams(raw))

const profileResult = (data: unknown) =>
    ({ data }) as unknown as ReturnType<typeof useAccountProfileQuery>

beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(useAccountProfileQuery).mockReturnValue(
        profileResult({ userid: "u-1" }),
    )
})

afterEach(() => {
    cleanup()
})

describe("useSalesOrdersListQuery", () => {
    it("用 URL 状态 + 登录人身份构建列表查询并取数", async () => {
        vi.mocked(fetchSalesOrders).mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            pageSize: 20,
            queriedAt: "2026-01-01T00:00:00Z",
        })
        const url = makeUrl(
            "page=2&pageSize=50&q=SO-1&nature=card_voucher&origin=mall&sort=amount&dir=desc&createdFrom=2026-01-02&createdTo=2026-01-03",
        )
        const { result } = renderHookWithProviders(
            () => useSalesOrdersListQuery(url),
            { queryClient: createFreshQueryClient() },
        )

        expect(result.current.ordersQuery.isPending).toBe(true)
        await waitFor(() => expect(fetchSalesOrders).toHaveBeenCalledTimes(1))

        expect(fetchSalesOrders).toHaveBeenCalledWith(
            expect.objectContaining({
                page: 2,
                pageSize: 50,
                search: "SO-1",
                nature: "card_voucher",
                summary: "all",
                currentUserId: "u-1",
                origin: "mall",
                sortBy: "amountGross",
                sortDir: "desc",
                createdFrom: Date.UTC(2026, 0, 2, 0, 0, 0) / 1000 - 8 * 3600,
                createdTo:
                    Date.UTC(2026, 0, 3, 23, 59, 59) / 1000 - 8 * 3600,
            }),
        )

        await waitFor(() =>
            expect(result.current.ordersQuery.data?.total).toBe(0),
        )
        expect(result.current.ordersQuery.isError).toBe(false)
    })

    it("未知排序列不映射 sortBy", async () => {
        vi.mocked(fetchSalesOrders).mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            pageSize: 20,
            queriedAt: "2026-01-01T00:00:00Z",
        })
        const url = makeUrl("sort=bogus&dir=asc")
        renderHookWithProviders(() => useSalesOrdersListQuery(url), {
            queryClient: createFreshQueryClient(),
        })

        await waitFor(() => expect(fetchSalesOrders).toHaveBeenCalledTimes(1))
        expect(fetchSalesOrders).toHaveBeenCalledWith(
            expect.objectContaining({ sortBy: undefined, sortDir: "asc" }),
        )
    })

    it("同一 URL 状态重渲染保持 queryKey 稳定，不重复请求", async () => {
        vi.mocked(fetchSalesOrders).mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            pageSize: 20,
            queriedAt: "2026-01-01T00:00:00Z",
        })
        const url = makeUrl("page=1")
        const { rerender } = renderHookWithProviders(
            () => useSalesOrdersListQuery(url),
            { queryClient: createFreshQueryClient() },
        )

        await waitFor(() => expect(fetchSalesOrders).toHaveBeenCalledTimes(1))
        rerender()
        expect(fetchSalesOrders).toHaveBeenCalledTimes(1)
    })

    it("待我处理视图在身份未就绪前不发起查询", async () => {
        vi.mocked(useAccountProfileQuery).mockReturnValue(
            profileResult(undefined),
        )
        const url = makeUrl("summary=mine")
        const { result } = renderHookWithProviders(
            () => useSalesOrdersListQuery(url),
            { queryClient: createFreshQueryClient() },
        )

        expect(result.current.identityReady).toBe(false)
        expect(result.current.ordersQuery.isPending).toBe(true)
        expect(fetchSalesOrders).not.toHaveBeenCalled()
    })

    it("身份就绪后待我处理视图按 currentUserId 取数", async () => {
        vi.mocked(useAccountProfileQuery).mockReturnValue(
            profileResult(undefined),
        )
        vi.mocked(fetchSalesOrders).mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            pageSize: 20,
            queriedAt: "2026-01-01T00:00:00Z",
        })
        const url = makeUrl("summary=mine")
        const { rerender } = renderHookWithProviders(
            () => useSalesOrdersListQuery(url),
            { queryClient: createFreshQueryClient() },
        )
        expect(fetchSalesOrders).not.toHaveBeenCalled()

        vi.mocked(useAccountProfileQuery).mockReturnValue(
            profileResult({ userid: "u-9" }),
        )
        rerender()

        await waitFor(() => expect(fetchSalesOrders).toHaveBeenCalledTimes(1))
        expect(fetchSalesOrders).toHaveBeenCalledWith(
            expect.objectContaining({
                summary: "mine",
                currentUserId: "u-9",
            }),
        )
    })

    it("请求失败时进入 error 状态", async () => {
        vi.mocked(fetchSalesOrders).mockRejectedValue(new Error("boom"))
        const url = makeUrl("page=1")
        const { result } = renderHookWithProviders(
            () => useSalesOrdersListQuery(url),
            { queryClient: createFreshQueryClient() },
        )

        await waitFor(() =>
            expect(result.current.ordersQuery.isError).toBe(true),
        )
        expect(result.current.ordersQuery.data).toBeUndefined()
    })
})
