import { beforeEach, expect, it, vi } from "vitest"
import { apiGet } from "@/lib/api"
import { fetchSalesOrders } from "./sales-orders-list"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => {
    vi.resetAllMocks()
    vi.mocked(apiGet).mockResolvedValue({
        items: [],
        total: 0,
        page: 1,
        page_size: 20,
        empty_reason: "no_scope",
        scope_version: "v1",
        policy_version: 1,
        organization_version: 1,
        scope_summary: "销售单当前负责人、业务组织及有效协作或参与范围",
        as_of: "2026-09-14T08:00:00Z",
        ownership_basis: "document_sales_owner",
    })
})

it("组织筛选进入请求并回传 emptyReason 与 scopeVersion", async () => {
    const result = await fetchSalesOrders({
        page: 1,
        pageSize: 20,
        orgUnitIds: "org-1",
        includeDescendants: true,
    })
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/sales-orders",
        expect.objectContaining({
            org_unit_ids: "org-1",
            include_descendants: true,
        }),
    )
    expect(result.emptyReason).toBe("no_scope")
    expect(result.scopeVersion).toBe("v1")
    expect(result.asOf).toBe("2026-09-14T08:00:00Z")
    expect(result.ownershipBasis).toBe("document_sales_owner")
    expect(result.queriedAt).toBe("2026-09-14T08:00:00Z")
})
