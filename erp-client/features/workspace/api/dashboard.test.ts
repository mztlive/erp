import { beforeEach, describe, expect, it, vi } from "vitest"

import type { AccountProfile } from "@/features/auth/api"
import type { TodayWorkspaceQuery } from "@/features/workspace/types"

const mocks = vi.hoisted(() => ({
    listApprovalInstances: vi.fn(),
    getWorkItemStats: vi.fn(),
    listWorkItems: vi.fn(),
}))

vi.mock("@/features/approval-workflow/api", () => ({
    listApprovalInstances: mocks.listApprovalInstances,
}))

vi.mock("@/features/work-items/api", () => ({
    getWorkItemStats: mocks.getWorkItemStats,
    listWorkItems: mocks.listWorkItems,
}))

import { fetchWorkspaceDashboard } from "./dashboard"

const profile: AccountProfile = {
    userid: "fukuan",
    account: "fukuan",
    name: "付款",
    subject: "admin:fukuan",
    role_ids: ["role-finance"],
    permissions: ["approval_instance:read"],
    account_kind: "admin",
}

const baseQuery: TodayWorkspaceQuery = {
    view: "inbox",
    sort: "priority_due",
    timezone: "Asia/Shanghai",
}

describe("fetchWorkspaceDashboard started approvals", () => {
    beforeEach(() => {
        mocks.listApprovalInstances.mockReset()
        mocks.getWorkItemStats.mockReset()
        mocks.listWorkItems.mockReset()
        mocks.getWorkItemStats.mockResolvedValue({
            assigned: 0,
            inbox: 0,
            overdue: 0,
            blocked: 0,
            as_of: 1_788_000_000,
        })
        mocks.listWorkItems.mockResolvedValue({ items: [], total: 0 })
        mocks.listApprovalInstances.mockResolvedValue({
            items: [
                {
                    instanceId: "instance-42",
                    status: "RUNNING",
                    currentRoundNo: 1,
                    currentNodeName: "采购复核",
                    currentAssigneeName: "采购1",
                    documentType: "payment_reversal",
                    documentId: "reversal-42",
                    documentLabel: "PCZ-000042",
                    processVersion: "2",
                    startedAt: 1_788_000_000,
                },
            ],
            total: 1,
        })
    })

    it("shows the initiator metric without approval administration permissions", async () => {
        const dashboard = await fetchWorkspaceDashboard(baseQuery, profile)
        const started = dashboard.metrics.find(
            (metric) => metric.key === "started",
        )

        expect(started).toMatchObject({ visible: true, count: 1 })
        expect(mocks.listApprovalInstances).toHaveBeenCalledWith({
            view: "started",
            cursor: undefined,
            limit: 1,
        })
    })

    it("maps a started payment reversal to its W12 tracking detail", async () => {
        const dashboard = await fetchWorkspaceDashboard(
            { ...baseQuery, view: "started" },
            profile,
        )

        expect(dashboard.items).toHaveLength(1)
        expect(dashboard.items[0]).toMatchObject({
            workItemId: "instance-42",
            businessObjectType: "payment_reversal",
            businessObjectId: "reversal-42",
            stableNumber: "PCZ-000042",
            statusLabel: "审批中",
            ownerUserLabel: "采购1",
            destinationWorkspaceId: "W12",
            handlerKey: "document_approval",
            allowedActions: ["VIEW"],
            listSummary: "采购复核 · 采购1",
            impactSummary:
                "审批通过前原付款保持不变；通过后系统追加冲正记录并回冲原付款。",
            nextActionHint: "可打开冲正详情查看完整审批进度与原付款。",
        })
        expect(dashboard.familyCounts).toBeUndefined()
    })
})
