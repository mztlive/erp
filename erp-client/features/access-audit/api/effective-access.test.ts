import { beforeEach, expect, test, vi } from "vitest"

import { apiGet } from "@/lib/api"
import { fetchEffectiveAccess } from "./effective-access"
import { groupDataScopes } from "@/features/access-audit/lib/effective-access-preview"

vi.mock("@/lib/api", () => ({ apiGet: vi.fn() }))
beforeEach(() => vi.resetAllMocks())

test("数据范围保留资源编码，供预览按类型归并", async () => {
    vi.mocked(apiGet).mockImplementation(async (path) => {
        if (path === "/admin/roles") {
            return [
                {
                    id: "sales",
                    name: "销售",
                    permissions: ["customer:list"],
                    created_at: 1700000000,
                },
            ] as never
        }
        if (path === "/admin/data-scopes") {
            return {
                items: [
                    {
                        id: "s1",
                        subject_type: "role",
                        subject_id: "sales",
                        scope_type: "self_owned",
                        scope_targets: [],
                        resource: "customer",
                        actions: ["list"],
                        version: 1,
                        created_at: 0,
                    },
                    {
                        id: "s2",
                        subject_type: "role",
                        subject_id: "sales",
                        scope_type: "collaborative",
                        scope_targets: [],
                        resource: "customer",
                        actions: ["list"],
                        version: 1,
                        created_at: 0,
                    },
                    {
                        id: "s3",
                        subject_type: "role",
                        subject_id: "sales",
                        scope_type: "self_owned",
                        scope_targets: [],
                        resource: "sales_order",
                        actions: ["list"],
                        version: 1,
                        created_at: 0,
                    },
                ],
                total: 3,
                page: 1,
                page_size: 100,
            } as never
        }
        return [] as never
    })
    const view = await fetchEffectiveAccess("ROLE", "sales")
    expect(view?.dataScopes.map((scope) => scope.resource)).toEqual([
        "customer",
        "customer",
        "sales_order",
    ])
    expect(
        groupDataScopes(view?.dataScopes ?? []).map((group) => group.label),
    ).toEqual(["本人负责", "协作参与"])
})
