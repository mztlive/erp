import { afterEach, describe, expect, it, vi } from "vitest"
import { cleanup, renderHook } from "@testing-library/react"

import {
    shouldRetainSalesOrderWorkItemId,
    useSalesOrderDetailUrlState,
} from "@/features/sales-orders/hooks/use-sales-order-detail-url-state"

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

const router = vi.hoisted(() => ({ replace: vi.fn() }))
vi.mock("next/navigation", () => ({
    useRouter: () => router,
    useSearchParams: () =>
        new URLSearchParams(
            "section=change-review&changeOrderId=old-change&workItemId=task-1&queueContextId=queue-1",
        ),
}))
afterEach(cleanup)

it("变更审核保留精确变更身份，离开分区清除变更上下文", () => {
    const { result } = renderHook(() =>
        useSalesOrderDetailUrlState({ salesOrderId: "sales-1" }),
    )
    expect(result.current.focusedChangeOrderId).toBe("old-change")
    result.current.selectSection("change-review")
    const change = new URL(router.replace.mock.lastCall![0], "http://erp.test")
    expect(change.searchParams.get("changeOrderId")).toBe("old-change")
    expect(change.searchParams.get("workItemId")).toBe("task-1")
    result.current.selectSection("overview")
    const overview = new URL(
        router.replace.mock.lastCall![0],
        "http://erp.test",
    )
    expect(overview.searchParams.has("changeOrderId")).toBe(false)
    expect(overview.searchParams.has("workItemId")).toBe(false)
})
