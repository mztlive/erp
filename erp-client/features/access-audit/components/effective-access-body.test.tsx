import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import { EffectiveAccessBody } from "./effective-access-body"
import {
    makeGovernancePolicies,
    makeRoleRow,
} from "@/features/access-audit/hooks/test-data"
import type { EffectiveAccessView } from "@/features/access-audit/types"
import type { useEffectiveAccessQuery } from "@/features/access-audit/hooks/queries"

afterEach(cleanup)

function grant(
    id: string,
    scopeType: "self_owned" | "collaborative",
    resource: string,
): EffectiveAccessView["dataScopes"][number] {
    return {
        id,
        layer: "DATA_SCOPE",
        layerLabel: "数据范围",
        targetLabel: scopeType === "self_owned" ? "本人负责" : "协作参与",
        capability: resource,
        sourceType: "ROLE",
        sourceLabel: "销售",
        resource,
        scopeType,
        scopeTargets: [],
    }
}

function query(
    data: EffectiveAccessView,
): ReturnType<typeof useEffectiveAccessQuery> {
    return {
        isPending: false,
        isError: false,
        data,
        error: null,
        refetch: vi.fn(),
    } as unknown as ReturnType<typeof useEffectiveAccessQuery>
}

const view: EffectiveAccessView = {
    subject: { type: "ROLE", id: "sales", label: "销售" },
    moduleAndActionGrants: [
        {
            id: "p1",
            layer: "MODULE_ACTION",
            layerLabel: "模块与动作权限",
            targetLabel: "客户 · 查看列表",
            capability: "customer:list",
            sourceType: "ROLE",
            sourceLabel: "销售",
        },
    ],
    dataScopes: [
        grant("1", "self_owned", "customer"),
        grant("2", "collaborative", "customer"),
        grant("3", "self_owned", "sales_order"),
        grant("4", "collaborative", "sales_order"),
        grant("5", "self_owned", "contract"),
        grant("6", "collaborative", "contract"),
    ],
    fieldPolicies: [],
    historicalParticipantRules: [],
    deniedOrBlocked: [],
    permissionVersion: "pv-live",
    calculatedAt: "2026-01-01T00:00:00.000Z",
    governancePolicies: makeGovernancePolicies(),
    allowedActions: ["VIEW_EFFECTIVE_ACCESS"],
    actionBlockers: [],
}

test("数据范围按类型汇总，不再逐条重复来源角色", () => {
    render(
        <EffectiveAccessBody
            query={query(view)}
            previewRole={makeRoleRow({
                name: "销售",
                permissionCount: 12,
                permissionGroups: [
                    { name: "客户", count: 7 },
                    { name: "销售单", count: 5 },
                ],
            })}
        />,
    )
    expect(screen.getAllByText("本人负责")).toHaveLength(1)
    expect(screen.getAllByText("协作参与")).toHaveLength(1)
    expect(screen.getByText(/可查看自己作为主责的单据/)).toBeTruthy()
    expect(screen.queryByText("来源 销售")).toBeNull()
    expect(screen.queryByText("数据范围来源")).toBeNull()
    expect(screen.queryByText("字段策略来源")).toBeNull()
    expect(screen.queryByText("历史参与者")).toBeNull()
    expect(screen.queryByText("覆盖模块")).toBeNull()
    expect(screen.getAllByText("客户").length).toBeGreaterThan(0)
})
