import { cleanup, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

vi.mock("@/features/purchase-orders/api/purchase-return-orders", () => ({
    fetchPurchaseReturnOrders: vi.fn(),
}))

import { fetchPurchaseReturnOrders } from "@/features/purchase-orders/api/purchase-return-orders"
import { renderHookWithProviders } from "@/features/test-utils"
import type { PurchaseReturnOrderRow } from "@/features/purchase-orders/types"
import { usePurchaseReturnOrdersQuery } from "./use-purchase-return-orders-query"

const mockedFetch = vi.mocked(fetchPurchaseReturnOrders)

const row = (): PurchaseReturnOrderRow => ({
    purchaseReturnOrderId: "pro-1",
    purchaseReturnNo: "TH-2026-001",
    purchaseOrderId: "po-1",
    returnMode: "company_warehouse_to_supplier",
    returnModeLabel: "公司仓退供应商",
    status: "pending_execution",
    statusLabel: "待执行",
    statusTone: "warning",
    version: 1,
    createdAt: "2023-11-14T22:13:20.000Z",
    allowedActions: ["VIEW_DETAIL"],
})

afterEach(() => {
    cleanup()
})

describe("usePurchaseReturnOrdersQuery", () => {
    beforeEach(() => {
        mockedFetch.mockReset()
    })

    it("loads related returns without attaching an approval projection", async () => {
        mockedFetch.mockResolvedValue([row()])
        const { result } = renderHookWithProviders(() =>
            usePurchaseReturnOrdersQuery("po-1"),
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.[0]?.statusLabel).toBe("待执行")
        expect("approval" in (result.current.data?.[0] ?? {})).toBe(false)
        expect(mockedFetch).toHaveBeenCalledWith("po-1")
    })

    it("surfaces a failure without inventing approval actions", async () => {
        mockedFetch.mockRejectedValue(new Error("forbidden"))
        const { result } = renderHookWithProviders(() =>
            usePurchaseReturnOrdersQuery("po-1"),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.data).toBeUndefined()
    })
})
