import { afterEach, expect, it, vi } from "vitest"
import { fetchCustomerDirectory } from "./directory"

const apiGet = vi.fn()

vi.mock("@/lib/api", () => ({
    apiGet: (...args: unknown[]) => apiGet(...args),
}))

afterEach(() => {
    apiGet.mockReset()
})

it("把范围版本和组织筛选传给服务端，并用 empty_reason 区分无范围", async () => {
    apiGet.mockResolvedValue({
        items: [],
        total: 0,
        page: 2,
        page_size: 20,
        owner_options: [],
        empty_reason: "no_scope",
        scope_version: "v1",
        policy_version: 3,
        organization_version: 4,
        scope_summary: "客户当前主负责人、协作关系及负责人所属组织范围",
    })
    const result = await fetchCustomerDirectory({
        scope: "all_authorized",
        status: "active",
        ownerUserIds: "user-1",
        orgUnitIds: "org-1",
        includeDescendants: true,
        page: 2,
        pageSize: 20,
        scopeVersion: "v1",
    })
    expect(apiGet).toHaveBeenCalledWith("/admin/customers", {
        scope: "all_authorized",
        scope_version: "v1",
        owner_user_ids: "user-1",
        org_unit_ids: "org-1",
        include_descendants: true,
        keyword: undefined,
        status: "active",
        page: 2,
        page_size: 20,
        sort_by: "updated_at",
        sort_dir: "desc",
    })
    expect(result.hasCustomerScope).toBe(false)
    expect(result.emptyReason).toBe("no_scope")
    expect(result.scopeVersion).toBe("v1")
    expect(result.policyVersion).toBe(3)
    expect(result.organizationVersion).toBe(4)
})
