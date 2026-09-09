import { afterEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { createApiError } from "@/lib/api/errors"
import { fetchCustomerAccountsDetail } from "./list-view"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
afterEach(() => vi.resetAllMocks())
it.each(["receivable", "receipt", "invoice", "refund", "reversal"] as const)(
    "%s 详情只将 404 作为不存在，服务异常和权限失败必须透传",
    async (kind) => {
        vi.mocked(apiGet).mockRejectedValue(
            createApiError({ kind: "Http", status: 404, message: "不存在" }),
        )
        expect(await fetchCustomerAccountsDetail(kind, "id-1")).toBeNull()
        for (const status of [403, 500]) {
            const error = createApiError({
                kind: "Http",
                status,
                message: "读取失败",
            })
            vi.mocked(apiGet).mockRejectedValue(error)
            await expect(
                fetchCustomerAccountsDetail(kind, "id-1"),
            ).rejects.toBe(error)
        }
    },
)
