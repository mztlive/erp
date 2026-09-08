import { beforeEach, describe, expect, it, vi } from "vitest"
import { apiPut } from "@/lib/api"
import type { CreateRevisionInput } from "@/features/master-data/types"
import { updateVoucherCategoryRevision } from "./voucher"

vi.mock("@/lib/api", () => ({ apiPut: vi.fn(), apiPost: vi.fn() }))
vi.mock("@/features/master-data/api/centers/voucher", () => ({
    centerVoucher: vi.fn(),
}))
const input: CreateRevisionInput = {
    resource: "voucher-categories",
    stableId: "sku-1",
    baseRevisionId: "rev-1",
    expectedLockVersion: 7,
    name: "体验卡",
    effectiveFrom: "2026-09-08",
    changeReason: "停用",
    fields: { voucherNo: "VC-1", description: "原始描述" },
    idempotencyKey: "status-1",
}
beforeEach(() => {
    vi.mocked(apiPut).mockReset()
})
describe("voucher status update", () => {
    it.each(["active", "disabled"] as const)(
        "submits explicit %s with the product lock and preserves content",
        async (status) => {
            vi.mocked(apiPut).mockResolvedValue({
                id: "rev-2",
                sku_id: "sku-1",
                sku_no: "VC-1",
                revision_no: 2,
                status,
            })
            expect(
                (await updateVoucherCategoryRevision(input, status)).outcome,
            ).toBe("succeeded")
            expect(apiPut).toHaveBeenCalledWith(
                "/admin/voucher-categories/sku-1",
                expect.objectContaining({
                    status,
                    version: 7,
                    name: "体验卡",
                    description: "原始描述",
                }),
            )
        },
    )
    it("does not change status when editing normal category content", async () => {
        vi.mocked(apiPut).mockResolvedValue({
            id: "rev-2",
            sku_id: "sku-1",
            revision_no: 2,
            status: "disabled",
        })
        await updateVoucherCategoryRevision(input)
        expect(vi.mocked(apiPut).mock.calls[0][1]).not.toHaveProperty("status")
    })
    it("reports conflicts and never claims success if the server ignores status", async () => {
        vi.mocked(apiPut).mockRejectedValueOnce({
            kind: "Http",
            status: 409,
            message: "版本冲突",
        })
        expect(
            (await updateVoucherCategoryRevision(input, "disabled")).outcome,
        ).toBe("conflict")
        vi.mocked(apiPut).mockResolvedValue({
            id: "rev-2",
            sku_id: "sku-1",
            revision_no: 2,
            status: "active",
        })
        expect(
            (await updateVoucherCategoryRevision(input, "disabled")).outcome,
        ).toBe("blocked")
    })
})
