import { beforeEach, describe, expect, it, vi } from "vitest"

import type { BackendPurchaseReturnOrder } from "./purchase-return-order-wire-types"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
}))

import { apiGet } from "@/lib/api"
import {
    fetchPurchaseReturnOrders,
    projectPurchaseReturnOrder,
} from "./purchase-return-orders"
import { purchaseReturnActionsExcludeApproval } from "@/features/purchase-orders/lib/purchase-return-order-no-approval"

const mockedGet = vi.mocked(apiGet)

function returnSeed(): BackendPurchaseReturnOrder {
    return {
        id: "pro-1",
        purchase_return_no: "TH-2026-001",
        purchase_order_id: "po-1",
        sales_return_case_id: null,
        return_mode: "company_warehouse_to_supplier",
        status: "pending_execution",
        version: 1,
        created_at: 1_700_000_000,
        lines: [],
    }
}

describe("projectPurchaseReturnOrder", () => {
    it("maps a pending-execution return without an approval projection", () => {
        const row = projectPurchaseReturnOrder(returnSeed())
        expect(row.purchaseReturnNo).toBe("TH-2026-001")
        expect(row.statusLabel).toBe("待执行")
        expect(row.statusLabel).not.toBe("审批中")
        expect(row.statusLabel).not.toBe("审批复核")
        expect(row.returnModeLabel).toBe("公司仓退供应商")
        expect("approval" in row).toBe(false)
        expect(purchaseReturnActionsExcludeApproval(row.allowedActions)).toBe(
            true,
        )
    })

    it("strips a stray approval field instead of rendering a binding", () => {
        const row = projectPurchaseReturnOrder({
            ...returnSeed(),
            approval: {
                requirement: "PROCESS_REQUIRED",
                allowed_actions: ["SUBMIT", "APPROVE"],
            },
        } as BackendPurchaseReturnOrder & { approval: unknown })
        expect("approval" in row).toBe(false)
        expect(row.statusLabel).toBe("待执行")
        expect(row.purchaseReturnOrderId).toBe("pro-1")
    })
})

describe("fetchPurchaseReturnOrders", () => {
    beforeEach(() => {
        mockedGet.mockReset()
    })

    it("projects listed returns without an approval zone", async () => {
        mockedGet.mockResolvedValue({
            items: [returnSeed()],
            total: 1,
            page: 1,
            page_size: 100,
        })
        const rows = await fetchPurchaseReturnOrders("po-1")
        expect(mockedGet).toHaveBeenCalledWith(
            "/admin/purchase-return-orders",
            expect.objectContaining({ purchase_order_id: "po-1" }),
        )
        expect(rows).toHaveLength(1)
        expect(rows[0]?.statusLabel).toBe("待执行")
        expect("approval" in (rows[0] ?? {})).toBe(false)
    })
})
