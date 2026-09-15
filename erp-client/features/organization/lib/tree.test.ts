import { expect, it } from "vitest"

import { PAGE_NARROW_CLASS } from "./labels"
import { buildOrganizationForest, personLabel } from "./tree"
import type { OrganizationStateView } from "@/features/organization/types"

const view: OrganizationStateView = {
    version: 1,
    organizationVersion: 1,
    policyVersion: 1,
    scopeVersion: "v",
    asOf: "2026-09-15T00:00:00Z",
    emptyReason: null,
    scopeSummary: "组织配置",
    ownershipBasis: "org_unit_configuration",
    people: [
        {
            id: "u1",
            label: "张三",
            account: "zhang",
            active: false,
            own_org_unit_id: null,
        },
    ],
    roles: [],
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
        {
            id: "one",
            name: "一组",
            parent_id: "sales",
            kind: "team",
            enabled: true,
            version: 1,
            reason: "初始化",
        },
    ],
    memberships: [],
    management: [],
}

it("按名称筛选仍保留可见祖先，人员展示不含内部 ID", () => {
    const forest = buildOrganizationForest(view, {
        q: "一组",
        kind: "all",
        status: "all",
    })
    expect(forest).toHaveLength(1)
    expect(forest[0]?.unit.name).toBe("销售部")
    expect(forest[0]?.children[0]?.unit.name).toBe("一组")
    expect(personLabel(view.people, "u1")).toContain("已停用")
    expect(personLabel(view.people, "u1")).not.toContain("u1")
})

it("窄屏页面类禁止横向溢出", () => {
    expect(PAGE_NARROW_CLASS).toContain("overflow-x-hidden")
    expect(PAGE_NARROW_CLASS).toContain("min-w-0")
})

it("只展示 as_of 有效期内的成员和管理授权", () => {
    const forest = buildOrganizationForest(
        {
            ...view,
            memberships: [
                {
                    id: "current",
                    user_id: "u1",
                    org_unit_id: "sales",
                    valid_from: 1,
                    valid_to: null,
                    reason: "在职",
                },
                {
                    id: "ended",
                    user_id: "u1",
                    org_unit_id: "sales",
                    valid_from: 1,
                    valid_to: 10,
                    reason: "已结束",
                },
            ],
            management: [
                {
                    id: "grant-ended",
                    user_id: "u1",
                    role_id: "role-sales",
                    org_unit_id: "sales",
                    include_descendants: false,
                    valid_from: 1,
                    valid_to: 10,
                    reason: "已撤销",
                },
            ],
        },
        { kind: "all", status: "all" },
    )
    expect(forest[0]?.members.map((item) => item.id)).toEqual(["current"])
    expect(forest[0]?.management).toEqual([])
})
