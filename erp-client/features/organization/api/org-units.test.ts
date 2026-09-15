import { beforeEach, expect, it, vi } from "vitest"

import { apiGet, apiPost } from "@/lib/api"
import {
    fetchOrganizationState,
    previewOrganizationChange,
    submitOrganizationChange,
} from "./org-units"

vi.mock("@/lib/api", () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

beforeEach(() => vi.resetAllMocks())

const state = {
    version: 3,
    units: [
        {
            id: "sales",
            name: "销售部",
            parent_id: null,
            kind: "department",
            enabled: true,
            version: 1,
            reason: "初始化",
        },
    ],
    memberships: [],
    management: [],
    people: [
        {
            id: "u1",
            label: "张三",
            account: "zhang",
            active: true,
            own_org_unit_id: "sales",
        },
    ],
    roles: [{ id: "role-sales", name: "销售", enabled: true }],
    scope_version: "scope-v1",
    policy_version: 8,
    organization_version: 3,
    as_of: "2026-09-15T00:00:00Z",
    empty_reason: "no_scope",
    scope_summary: "组织配置边界内的内部组织、成员与管理关系",
    ownership_basis: "org_unit_configuration",
}

it("组织查询回传 emptyReason、scopeVersion 与组织版本", async () => {
    vi.mocked(apiGet).mockResolvedValue(state)
    const result = await fetchOrganizationState()
    expect(apiGet).toHaveBeenCalledWith("/admin/org-units")
    expect(result.emptyReason).toBe("no_scope")
    expect(result.scopeVersion).toBe("scope-v1")
    expect(result.organizationVersion).toBe(3)
    expect(result.policyVersion).toBe(8)
    expect(result.people[0]?.label).toBe("张三")
})

it("预览与提交携带 expected_version 且使用相同命令", async () => {
    const request = {
        expected_version: 3,
        idempotency_key: "idem-1",
        reason: "成员调岗",
        change: {
            operation: "transfer_member" as const,
            user_id: "u1",
            org_unit_id: "sales",
        },
    }
    vi.mocked(apiPost).mockResolvedValue({
        id: "receipt-1",
        actor_id: "admin",
        request,
        before: state,
        after: state,
        as_of: 1_000,
    })
    await previewOrganizationChange(request)
    await submitOrganizationChange(request)
    expect(apiPost).toHaveBeenNthCalledWith(
        1,
        "/admin/org-units/preview",
        request,
    )
    expect(apiPost).toHaveBeenNthCalledWith(
        2,
        "/admin/org-units/change",
        request,
    )
    expect(request.expected_version).toBe(3)
})
