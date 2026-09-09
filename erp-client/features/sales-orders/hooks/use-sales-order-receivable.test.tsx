import type { ReactNode } from "react"
import { afterEach, expect, it, vi } from "vitest"
import { cleanup, renderHook, waitFor } from "@testing-library/react"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { useSalesOrderReceivable } from "./use-sales-order-receivable"
import {
    fetchOrderReceivables,
    fetchOrderReceipts,
    fetchOrderInvoices,
} from "../api/sales-order-finance"
import type { SalesOrderDetailView } from "../api/sales-orders"

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: () => ({
        data: {
            permissions: [
                "receivable_account:list",
                "customer_receipt:list",
                "invoice:list",
            ],
        },
        isError: false,
    }),
}))
vi.mock("../api/sales-order-finance", () => ({
    fetchOrderReceivables: vi.fn(),
    fetchOrderReceipts: vi.fn(),
    fetchOrderInvoices: vi.fn(),
}))
afterEach(cleanup)

it("从财务办理页返回时立即重读票款，使用财务缓存失效范围", async () => {
    const client = new QueryClient({
        defaultOptions: { queries: { retry: false, staleTime: 60_000 } },
    })
    const wrapper = ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    )
    vi.mocked(fetchOrderReceivables).mockResolvedValue([])
    vi.mocked(fetchOrderInvoices).mockResolvedValue({
        items: [],
        total: 0,
        page: 1,
        page_size: 20,
    })
    vi.mocked(fetchOrderReceipts).mockResolvedValue({
        items: [],
        total: 0,
        page: 1,
        page_size: 20,
    })
    const order = {
        id: "so-1",
        nature: "physical_service",
    } as SalesOrderDetailView
    const first = renderHook(() => useSalesOrderReceivable(order), { wrapper })
    await waitFor(() =>
        expect(first.result.current.receipts.isSuccess).toBe(true),
    )
    first.unmount()
    vi.mocked(fetchOrderReceipts).mockResolvedValue({
        items: [],
        total: 1,
        page: 1,
        page_size: 20,
    })
    const second = renderHook(() => useSalesOrderReceivable(order), { wrapper })
    await waitFor(() =>
        expect(second.result.current.receipts.data?.total).toBe(1),
    )
    expect(fetchOrderReceipts).toHaveBeenCalledTimes(2)
    expect(
        client
            .getQueryCache()
            .findAll({ queryKey: ["customer-receivables", "finance", "so-1"] }),
    ).toHaveLength(3)
    expect(client.getMutationCache().getAll()).toHaveLength(0)
})
