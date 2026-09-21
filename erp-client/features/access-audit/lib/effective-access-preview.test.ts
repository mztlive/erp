import { expect, test } from "vitest"

import { resourceLabel } from "@/features/admin/lib/permission-catalog"
import type { EffectiveAccessView } from "@/features/access-audit/types"
import {
    formatResourceList,
    groupDataScopes,
    permissionPreview,
} from "./effective-access-preview"

function grant(
    overrides: Partial<EffectiveAccessView["dataScopes"][number]> & {
        id: string
        targetLabel: string
    },
): EffectiveAccessView["dataScopes"][number] {
    return {
        layer: "DATA_SCOPE",
        layerLabel: "数据范围",
        capability: overrides.resource ?? "",
        sourceType: "ROLE",
        sourceLabel: "销售",
        ...overrides,
    }
}

test("同一类型的多条数据范围归并成一组，并列出适用对象", () => {
    const groups = groupDataScopes([
        grant({
            id: "1",
            targetLabel: "本人负责",
            scopeType: "self_owned",
            resource: "customer",
        }),
        grant({
            id: "2",
            targetLabel: "本人负责",
            scopeType: "self_owned",
            resource: "sales_order",
        }),
        grant({
            id: "3",
            targetLabel: "协作参与",
            scopeType: "collaborative",
            resource: "customer",
        }),
        grant({
            id: "4",
            targetLabel: "协作参与",
            scopeType: "collaborative",
            resource: "customer",
        }),
        grant({
            id: "5",
            targetLabel: "协作参与",
            scopeType: "collaborative",
            resource: "sales_order",
        }),
    ])
    expect(groups.map((group) => group.label)).toEqual(["本人负责", "协作参与"])
    expect(groups[0]?.explanation).toContain("主责")
    expect(groups[1]?.explanation).toContain("协作人")
    const expected = [
        resourceLabel("customer"),
        resourceLabel("sales_order"),
    ].sort((a, b) => a.localeCompare(b, "zh-CN"))
    expect(groups[0]?.resources).toEqual(expected)
    expect(groups[1]?.resources).toEqual(expected)
})

test("公司级排在本人负责之前，指定目标只计数不展示内部编号", () => {
    const groups = groupDataScopes([
        grant({
            id: "self",
            targetLabel: "本人负责",
            scopeType: "self_owned",
            resource: "customer",
        }),
        grant({
            id: "org",
            targetLabel: "组织",
            scopeType: "organization",
            resource: "product",
            scopeTargets: ["org-east", "org-south"],
        }),
    ])
    expect(groups.map((group) => group.scopeType)).toEqual([
        "organization",
        "self_owned",
    ])
    expect(groups[0]?.specifiedTargetCount).toBe(2)
    expect(groups[0]?.resources.join()).not.toContain("org-east")
})

test("操作权限优先使用列表行已归并的模块摘要", () => {
    const preview = permissionPreview({
        previewRole: {
            id: "role-1",
            roleCode: "role_code_1",
            name: "管理员",
            status: "enabled",
            statusLabel: "启用",
            statusTone: "success",
            permissionSummary: "共 12 项",
            permissionCount: 12,
            permissionGroups: [
                { name: "客户", count: 7 },
                { name: "销售单", count: 5 },
            ],
            allPermissions: false,
            boundAccountCount: 3,
            dataScopeSummary: "公司级",
            fieldPolicySummary: "—",
            riskFlags: [],
            permissionVersion: "pv-live",
            organizationLabel: "总部",
        },
        grants: [
            {
                id: "ignored",
                layer: "MODULE_ACTION",
                layerLabel: "模块与动作权限",
                targetLabel: "客户 · 查看列表",
                capability: "customer:list",
                sourceType: "ROLE",
                sourceLabel: "管理员",
            },
        ],
    })
    expect(preview.count).toBe(12)
    expect(preview.groups.map((group) => group.name)).toEqual([
        "客户",
        "销售单",
    ])
})

test("资源列表超过 8 项时收成一句", () => {
    const labels = [
        "客户",
        "合同",
        "销售单",
        "选品册",
        "采购单",
        "供应商",
        "商品",
        "库存",
        "履约",
    ]
    expect(formatResourceList(labels)).toBe(
        "客户、合同、销售单、选品册、采购单、供应商 等 9 类对象",
    )
})
