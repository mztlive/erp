import { beforeEach, expect, it, vi } from "vitest"

import { apiGet } from "@/lib/api"
import { fetchBooks } from "@/features/sales-selection/api/books"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))

beforeEach(() => {
    vi.resetAllMocks()
})

it("列表携带负责人与组织范围并解析列表与版本", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        page: {
            items: [
                {
                    id: "book-1",
                    book_id: "book-1",
                    version: 1,
                    customer_id: "cust-1",
                    customer_name: "测试客户",
                    sales_owner_user_id: "user-1",
                    business_org_unit_id: "org-1",
                    form: "SINGLE_SKU",
                    selection_form: "SINGLE_SKU",
                    submit_mode: "BY_QUANTITY",
                    status: "PUBLISHED",
                    proposal_id: null,
                    created_at: 1,
                },
            ],
            total: 1,
            page: 1,
            page_size: 20,
        },
        scope_version: "v1",
        policy_version: 3,
        organization_version: 5,
        no_scope: false,
    })
    const result = await fetchBooks({
        owner_user_ids: "user-1",
        org_unit_ids: "org-1",
        include_descendants: true,
        page: 2,
        scope_version: "v1",
    })
    expect(apiGet).toHaveBeenCalledWith(
        "/admin/sales-selection-books",
        expect.objectContaining({
            owner_user_ids: "user-1",
            org_unit_ids: "org-1",
            include_descendants: true,
            scope_version: "v1",
            page: 2,
        }),
    )
    expect(result.rows).toHaveLength(1)
    expect(result.rows[0].sales_owner_user_id).toBe("user-1")
    expect(apiGet).toHaveBeenCalledTimes(1)
    expect(result.scopeVersion).toBe("v1")
    expect(result.noScope).toBe(false)
})

it("无范围时透出标记供页面与筛空区分", async () => {
    vi.mocked(apiGet).mockResolvedValue({
        page: { items: [], total: 0, page: 1, page_size: 20 },
        scope_version: "v2",
        policy_version: 3,
        organization_version: 5,
        no_scope: true,
    })
    const result = await fetchBooks({ page: 1 })
    expect(result.rows).toEqual([])
    expect(result.noScope).toBe(true)
})
