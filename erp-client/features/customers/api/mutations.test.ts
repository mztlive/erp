import { afterEach, expect, it, vi } from "vitest"

import { ApiErrorException } from "@/lib/api/errors"
import { createCustomer } from "./mutations"

const apiPost = vi.fn()

vi.mock("@/lib/api", async (importOriginal) => {
    const actual = await importOriginal<typeof import("@/lib/api")>()
    return {
        ...actual,
        apiPost: (...args: unknown[]) => apiPost(...args),
        apiGet: vi.fn(),
        apiPut: vi.fn(),
    }
})

afterEach(() => {
    apiPost.mockReset()
})

it("把缺少主属组织的 400 展示为拒绝结果，而不是抛错", async () => {
    apiPost.mockRejectedValue(
        new ApiErrorException({
            kind: "Validation",
            message: "请先维护负责人的有效主属组织",
            status: 400,
            code: "INVALID_REQUEST",
        }),
    )
    const result = await createCustomer({
        legalName: "示例客户",
        unifiedCreditCode: "91110105MA00CRBJ0X",
        defaultPaymentTerm: "POSTPAY_NET15",
        idempotencyKey: "create-1",
    })
    expect(result).toEqual({
        outcome: "rejected",
        message: "请先维护负责人的有效主属组织",
    })
})
