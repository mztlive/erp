import { expect, it } from "vitest"

import { impactChanges } from "./impact"
import type { OrganizationChangeReceipt } from "@/features/organization/types"

const unit = {
    id: "sales",
    name: "销售部",
    parent_id: null,
    kind: "department" as const,
    enabled: true,
    version: 1,
    reason: "初始化",
}

it("预览展示调岗前后组织且不宣称改派任务", () => {
    const receipt: OrganizationChangeReceipt = {
        id: "r1",
        actor_id: "admin",
        request: {
            expected_version: 1,
            idempotency_key: "k",
            reason: "调岗",
            change: {
                operation: "transfer_member",
                user_id: "u1",
                org_unit_id: "two",
            },
        },
        before: {
            version: 1,
            units: [unit, { ...unit, id: "two", name: "二组" }],
            memberships: [
                {
                    id: "m1",
                    user_id: "u1",
                    org_unit_id: "sales",
                    valid_from: 1,
                    valid_to: null,
                    reason: "入职",
                },
            ],
            management: [],
        },
        after: {
            version: 2,
            units: [unit, { ...unit, id: "two", name: "二组" }],
            memberships: [
                {
                    id: "m2",
                    user_id: "u1",
                    org_unit_id: "two",
                    valid_from: 2,
                    valid_to: null,
                    reason: "调岗",
                },
            ],
            management: [],
        },
        as_of: 2,
    }
    const changes = impactChanges(receipt, {
        people: [
            {
                id: "u1",
                label: "张三",
                account: "zhang",
                active: true,
                own_org_unit_id: "two",
            },
        ],
        roles: [],
    })
    const text = JSON.stringify(changes)
    expect(text).toContain("张三")
    expect(text).toContain("二组")
    expect(text).toContain("不改派任务")
    expect(text).not.toContain("自动改派")
})
