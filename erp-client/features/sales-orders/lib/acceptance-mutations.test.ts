import { beforeEach, describe, expect, it, vi } from "vitest"

import { apiPost } from "@/lib/api"
import {
    postCustomerAcceptanceWorkspace,
    reverseCustomerAcceptanceWorkspace,
} from "@/features/sales-orders/lib/acceptance-mutations"

vi.mock("@/lib/api", () => ({ apiPost: vi.fn() }))

const apiPostMock = vi.mocked(apiPost)

describe("customer acceptance command contract", () => {
    beforeEach(() => {
        apiPostMock.mockReset()
    })

    it("登记只发送操作号，业务验收单号由后端生成", async () => {
        apiPostMock.mockResolvedValue({
            acceptance: {
                id: "acceptance-1",
                acceptance_no: "CA20260831-000001",
                sales_order_id: "sales-order-1",
                accepted_at: 1_788_112_800,
                result: "PASSED",
                status: "POSTED",
                version: 2,
                created_at: 1_788_112_800,
            },
            remaining_eligibility: {
                sales_order_id: "sales-order-1",
                sales_lines: [],
                history: [],
            },
        })

        const result = await postCustomerAcceptanceWorkspace({
            salesOrderId: "sales-order-1",
            acceptanceDraftId: "draft-command-1",
            expectedDraftVersion: 0,
            expectedSalesOrderLockVersion: 3,
            idempotencyKey: "acceptance-command-1",
            acceptedAt: "2026-08-31T10:00:00.000Z",
            comment: "",
            lines: [],
        })

        const payload = apiPostMock.mock.calls[0]?.[1] as Record<
            string,
            unknown
        >
        expect(apiPostMock).toHaveBeenCalledWith(
            "/admin/customer-acceptances/commit",
            expect.objectContaining({
                idempotency_key: "acceptance-command-1",
            }),
        )
        expect(payload).not.toHaveProperty("acceptance_no")
        expect(result).toMatchObject({
            status: "succeeded",
            acceptanceNo: "CA20260831-000001",
        })
    })

    it("冲正发送稳定操作号并保留原业务单号用于结果展示", async () => {
        apiPostMock.mockResolvedValue({
            acceptance: {
                id: "acceptance-reverse",
                acceptance_no: "REV-CA20260831-000001",
                sales_order_id: "sales-order-1",
                accepted_at: 1_788_112_900,
                result: "REJECTED",
                status: "POSTED",
                version: 2,
                created_at: 1_788_112_900,
            },
            lines: [],
            allocations: [],
        })

        const result = await reverseCustomerAcceptanceWorkspace({
            salesOrderId: "sales-order-1",
            acceptanceId: "acceptance-original",
            originalAcceptanceNo: "CA20260831-000001",
            expectedAcceptanceVersion: 2,
            reasonText: "误录",
            idempotencyKey: "reverse-command-1",
        })

        expect(apiPostMock).toHaveBeenCalledWith(
            "/admin/customer-acceptances/acceptance-original/reverse",
            {
                expected_version: 2,
                reason_text: "误录",
                idempotency_key: "reverse-command-1",
            },
        )
        expect(result).toMatchObject({
            status: "succeeded",
            originalAcceptanceNo: "CA20260831-000001",
        })
    })
})
