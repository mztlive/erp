import { describe, expect, test, vi } from "vitest"

import {
    fetchSalesHandoverCandidates,
    submitSalesHandover,
} from "@/features/sales-orders/api/sales-orders-handover"
import {
    salesHandoverKeys,
    salesHandoverPreviewKey,
} from "@/features/sales-orders/hooks/use-sales-handover"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(async () => [
        { user_id: "sales-b", display_name: "李四", account: "lisiyong" },
    ]),
    apiPost: vi.fn(async () => ({
        sales_order_id: "so-1",
        sales_owner_user_id: "sales-b",
        business_org_unit_id: "org-a",
        version: 3,
        transferred_acceptance_task_ids: ["wi-1"],
        kept_approval_task_count: 1,
    })),
}))

describe("S3-07 销售责任交接", () => {
    test("候选只映射合格有效人员字段", async () => {
        const rows = await fetchSalesHandoverCandidates("so-1")
        expect(rows).toEqual([
            { userId: "sales-b", displayName: "李四", account: "lisiyong" },
        ])
    })

    test("交接请求使用显式目标与幂等键", async () => {
        const { apiPost } = await import("@/lib/api")
        const result = await submitSalesHandover({
            salesOrderId: "so-1",
            expectedVersion: 2,
            targetOwnerUserId: "sales-b",
            reason: "离职交接",
            idempotencyKey: "handover-key-1",
        })
        expect(result.salesOwnerUserId).toBe("sales-b")
        expect(result.transferredAcceptanceTaskIds).toEqual(["wi-1"])
        expect(result.keptApprovalTaskCount).toBe(1)
        expect(vi.mocked(apiPost).mock.calls[0]?.[1]).toMatchObject({
            expected_version: 2,
            target_owner_user_id: "sales-b",
            reason: "离职交接",
            idempotency_key: "handover-key-1",
        })
    })

    test("交接 QueryKey 与详情面路径一致", () => {
        expect(salesHandoverKeys.candidates("so-1")).toEqual([
            ...salesHandoverKeys.all,
            "candidates",
            "so-1",
        ])
        expect(salesOrderKeys.detail("so-1")).toContain("so-1")
    })

    test("随转预览复用详情验收面 QueryKey", () => {
        expect(salesHandoverPreviewKey("so-1")).toEqual([
            ...salesOrderKeys.acceptanceRoot("so-1"),
            "readonly",
        ])
    })
})
