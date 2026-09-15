import { beforeEach, expect, it, vi } from "vitest"

import { apiPost } from "@/lib/api"
import { fetchCompleteList } from "@/lib/collect-pages"
import {
    createDataScope,
    dataScopeListQuery,
    fetchDataScopes,
} from "./data-scopes"

vi.mock("@/lib/api", () => ({
    apiPost: vi.fn(),
    apiDelete: vi.fn(),
}))
vi.mock("@/lib/collect-pages", () => ({
    fetchCompleteList: vi.fn(),
}))

beforeEach(() => vi.resetAllMocks())

it("范围列表把资源动作放进查询并从信封回传版本字段", async () => {
    vi.mocked(fetchCompleteList).mockResolvedValue({
        items: [
            {
                id: "scope-1",
                subject_type: "role",
                subject_id: "role-sales",
                scope_type: "company",
                scope_targets: [],
                schema_version: 2,
                resource: "sales_order",
                actions: ["list"],
                target_dimension: "internal_org",
                enabled: true,
                version: 1,
                created_at: 1,
            },
        ],
        total: 1,
        empty_reason: "no_scope",
        scope_version: "v2",
        policy_version: 4,
        organization_version: 7,
        as_of: "2026-09-15T00:00:00Z",
        scope_summary: "已接入 DataScope v2 的资源动作配置",
        ownership_basis: "data_scope_configuration",
    })
    const url = {
        q: undefined,
        resource: "sales_order",
        action: "list",
        subjectType: "role" as const,
        subjectId: "role-sales",
        scopeType: "all" as const,
    }
    expect(dataScopeListQuery(url)).toEqual({
        resource: "sales_order",
        action: "list",
        subject_type: "role",
        subject_id: "role-sales",
        scope_type: undefined,
    })
    const result = await fetchDataScopes(url)
    expect(fetchCompleteList).toHaveBeenCalledWith(
        "/admin/data-scopes",
        dataScopeListQuery(url),
    )
    expect(result.scopeVersion).toBe("v2")
    expect(result.emptyReason).toBe("no_scope")
    expect(result.policyVersion).toBe(4)
    expect(result.items[0]?.resource).toBe("sales_order")
    expect(result.items[0]?.actions).toEqual(["list"])
})

it("主体未选类型时不发送 subject_id", () => {
    expect(
        dataScopeListQuery({
            subjectType: "all",
            subjectId: "role-sales",
            scopeType: "company",
        }),
    ).toEqual({
        resource: undefined,
        action: undefined,
        subject_type: undefined,
        subject_id: undefined,
        scope_type: "company",
    })
})

it("创建范围按资源动作提交，不发送通配目标", async () => {
    vi.mocked(apiPost).mockResolvedValue({ id: "new" })
    await createDataScope({
        subjectType: "role",
        subjectId: "role-sales",
        scopeType: "company",
        resource: "org_unit",
        actions: ["list"],
        targetDimension: "internal_org",
        targetMode: null,
        includeDescendants: null,
        scopeTargets: [],
    })
    expect(apiPost).toHaveBeenCalledWith(
        "/admin/data-scopes",
        expect.objectContaining({
            schema_version: 2,
            resource: "org_unit",
            actions: ["list"],
            subject_id: "role-sales",
            scope_targets: [],
        }),
    )
})
