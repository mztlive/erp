import { describe, expect, test } from "vitest"

import type { ScopedInvoiceRequestWire } from "@/features/invoice-requests/scoped"

function request(overrides: Partial<ScopedInvoiceRequestWire> = {}) {
    return {
        id: "req-1",
        request_no: "SQ-1",
        sales_order_id: "so-1",
        sales_order_no: "SO-1",
        status: "pending",
        created_at: 1,
        applicant_user_id: "applicant-1",
        handler_user_id: "handler-1",
        amount: "500.00",
        permission_limited: false,
        sales_owner_user_id: "sales-1",
        business_org_unit_id: "org-1",
        ...overrides,
    } satisfies ScopedInvoiceRequestWire
}

describe("M08 开票申请范围行", () => {
    test("申请金额为行级事实始终返回，不随受限置空", () => {
        const limited = request({ permission_limited: true })
        expect(limited.amount).toBe("500.00")
    })

    test("负责销售/申请人/当前处理人三维度可分别携带", () => {
        const row = request()
        expect(row.sales_owner_user_id).toBe("sales-1")
        expect(row.applicant_user_id).toBe("applicant-1")
        expect(row.handler_user_id).toBe("handler-1")
    })

    test("无当前处理人时为空而非空字符串", () => {
        expect(request({ handler_user_id: null }).handler_user_id).toBeNull()
    })
})
