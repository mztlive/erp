import { beforeEach, describe, expect, it, vi } from "vitest"

const apiGet = vi.fn()

vi.mock("@/lib/api/client", () => ({
    apiGet: (...args: unknown[]) => apiGet(...args),
    apiPost: vi.fn(),
}))

import { fetchSupplierOfferings } from "./offerings"

describe("fetchSupplierOfferings", () => {
    beforeEach(() => {
        apiGet.mockReset()
        apiGet.mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            page_size: 50,
            empty_reason: null,
        })
    })

    it("把维护人与采购负责人筛选送进同一授权查询", async () => {
        await fetchSupplierOfferings({
            ownerUserIds: "user-1",
            procurementOwnerUserIds: "buyer-1",
            orgUnitIds: "org-1",
            includeDescendants: true,
            page: 1,
        })
        expect(apiGet).toHaveBeenCalledWith(
            "/admin/supplier-offerings",
            expect.objectContaining({
                owner_user_ids: "user-1",
                procurement_owner_user_ids: "buyer-1",
                org_unit_ids: "org-1",
                include_descendants: true,
            }),
        )
    })

    it("第二页回传 scope_version，不把创建人当作维护人", async () => {
        await fetchSupplierOfferings({
            ownerUserIds: "user-1",
            page: 2,
            scopeVersion: "v1:abc",
        })
        expect(apiGet).toHaveBeenCalledWith(
            "/admin/supplier-offerings",
            expect.objectContaining({
                owner_user_ids: "user-1",
                scope_version: "v1:abc",
                page: 2,
            }),
        )
        const payload = apiGet.mock.calls[0]?.[1] as Record<string, unknown>
        expect(payload).not.toHaveProperty("created_by_user_ids")
    })
})
