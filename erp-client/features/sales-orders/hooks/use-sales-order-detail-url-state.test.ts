import { describe, expect, it } from "vitest"

import { shouldRetainSalesOrderWorkItemId } from "@/features/sales-orders/hooks/use-sales-order-detail-url-state"

describe("shouldRetainSalesOrderWorkItemId", () => {
    it("进入客户验收时只保留 W06 任务上下文", () => {
        expect(shouldRetainSalesOrderWorkItemId("acceptance", true)).toBe(true)
        expect(shouldRetainSalesOrderWorkItemId("acceptance", false)).toBe(
            false,
        )
    })

    it("审批分区继续保留原有任务上下文", () => {
        expect(shouldRetainSalesOrderWorkItemId("approval", false)).toBe(true)
        expect(shouldRetainSalesOrderWorkItemId("change-review", false)).toBe(
            true,
        )
        expect(shouldRetainSalesOrderWorkItemId("overview", true)).toBe(false)
    })
})
