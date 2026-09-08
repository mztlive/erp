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

    it("shows frozen submission amounts including zero without replacing document detail facts", async () => {
        const { mapInstanceListItemDto } =
            await import("@/features/approval-workflow/types")
        mocks.listApprovalInstances.mockResolvedValue({
            items: ["12800.50", "0", null].map((amount, i) =>
                mapInstanceListItemDto({
                    instance_id: `instance-${i}`,
                    status: "RUNNING",
                    current_round_no: 1,
                    document_type: "purchase_order",
                    document_id: `po-${i}`,
                    total_amount: amount,
                }),
            ),
            total: 3,
        })
        const view = await fetchWorkspaceDashboard(
            { ...baseQuery, view: "started" },
            profile,
        )
        expect(view.items.map((item) => item.amountSummary?.value)).toEqual([
            "¥12,800.50",
            "¥0.00",
            undefined,
        ])
        expect(view.items[0].amountSummary?.label).toBe("提交金额")
        expect(view.items[0].summarySections).toBeUndefined()
    })

    it.each([
        "sales_order",
        "voucher_sales_order",
        "sales_change_order",
        "purchase_order",
        "purchase_change_order",
        "stock_adjustment",
        "customer_receipt",
        "customer_refund",
        "receipt_reversal",
        "supplier_refund",
        "payment_reversal",
    ])("%s 的发起人展示完整单据摘要但只允许查看", async (documentType) => {
        const { mapInstanceListItemDto } =
            await import("@/features/approval-workflow/types")
        mocks.listApprovalInstances.mockResolvedValue({
            items: [
                mapInstanceListItemDto({
                    instance_id: "approval-1",
                    status: "RUNNING",
                    current_round_no: 1,
                    document_type: documentType,
                    document_id: "doc-1",
                    subject_version: 1,
                    document_label: "PO1",
                    total_amount: "920",
                    document_summary: {
                        root_business_object_id: "parent-1",
                        counterparty_label: "杭州狮峰茶叶有限公司",
                        impact_summary: "审批后生效",
                        list_summary: "供应商 · ¥920",
                        brief_more_count: 0,
                        summary_sections: [
                            {
                                label: "含税金额",
                                value: "¥920",
                                numeric: true,
                            },
                            {
                                label: "不含税金额",
                                value: "¥800.4",
                                numeric: true,
                            },
                            {
                                label: "税额",
                                value: "¥119.6",
                                numeric: true,
                            },
                            { label: "付款条件", value: "先款 50%" },
                        ],
                        brief_lines: [
                            {
                                title: "狮峰明前龙井礼盒 250g",
                                quantity: "1 盒 · ¥920",
                                due_label: "9/8 交",
                            },
                        ],
                    },
                }),
            ],
            total: 1,
        })
        const view = await fetchWorkspaceDashboard(
            { ...baseQuery, view: "started" },
            profile,
        )
        expect(view.items[0]).toMatchObject({
            rootBusinessObjectId: "parent-1",
            counterpartyName: "杭州狮峰茶叶有限公司",
            subjectVersion: "1",
            allowedActions: ["VIEW"],
            summarySections: expect.arrayContaining([
                expect.objectContaining({
                    label: "税额",
                    value: "¥119.6",
                    numeric: true,
                }),
            ]),
            briefLines: [
                {
                    title: "狮峰明前龙井礼盒 250g",
                    quantity: "1 盒 · ¥920",
                    dueLabel: "9/8 交",
                },
            ],
        })
        expect(view.items[0].amountSummary).toBeUndefined()
        expect(view.items[0].taskVersion).toBe("")
    })

    it("服务端明确缺少提交摘要时不把当前单据当作旧版本事实", async () => {
        const { mapInstanceListItemDto } =
            await import("@/features/approval-workflow/types")
        mocks.listApprovalInstances.mockResolvedValue({
            items: [
                mapInstanceListItemDto({
                    instance_id: "history",
                    status: "APPROVED",
                    current_round_no: 1,
                    document_type: "sales_order",
                    document_id: "so",
                    subject_version: 1,
                    document_summary: null,
                }),
            ],
            total: 1,
        })
        const view = await fetchWorkspaceDashboard(
            { ...baseQuery, view: "started" },
            profile,
        )
        expect(view.items[0].documentSummaryResolved).toBe(true)
        expect(view.items[0].summarySections).toBeUndefined()
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

    it("passes the search query when listing started approvals", async () => {
        await fetchWorkspaceDashboard(
            { ...baseQuery, view: "started", query: "PCZ-000042" },
            profile,
        )

        expect(mocks.listApprovalInstances).toHaveBeenCalledWith({
            view: "started",
            cursor: undefined,
            limit: 1,
        })
        expect(mocks.listApprovalInstances).toHaveBeenCalledWith({
            view: "started",
            cursor: undefined,
            limit: 20,
            query: "PCZ-000042",
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

describe("fetchWorkspaceDashboard managed queue", () => {
    const manager: AccountProfile = {
        ...profile,
        userid: "admin",
        account: "admin",
        name: "系统管理员",
        role_ids: ["role-root"],
        permissions: ["*:*"],
    }

    const managedItem = {
        id: "wi-managed",
        work_item_type: "PROCUREMENT_ORDER_CREATION",
        handler_key: "procurement_order_creation",
        approval_step_instance_id: null,
        status: "OPEN",
        assignment_source: "SYSTEM_RULE",
        owner_role: "role-procurement",
        owner_organization_id: "company",
        owner_user_id: "caigou",
        processing_state: "READY",
        business_object_type: "sales_order",
        business_object_id: "so-1",
        root_business_object_id: "so-1",
        business_object_label: "XS1",
        subject_version: "1",
        task_version: "1",
        priority: 2,
        created_at: 1_788_000_000,
        allowed_actions: ["VIEW", "REASSIGN"],
    }

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
        mocks.listApprovalInstances.mockResolvedValue({ items: [], total: 0 })
    })

    it("shows the managed metric and count for work_item:manage", async () => {
        mocks.listWorkItems.mockImplementation(
            async (input: { scope?: string; pageSize?: number }) => {
                if (input.scope === "managed") {
                    return { items: [managedItem], total: 2 }
                }
                return { items: [], total: 0 }
            },
        )

        const dashboard = await fetchWorkspaceDashboard(baseQuery, manager)
        const managed = dashboard.metrics.find(
            (metric) => metric.key === "managed",
        )

        expect(managed).toMatchObject({
            visible: true,
            count: 2,
            label: "范围内待办",
        })
        expect(mocks.listWorkItems).toHaveBeenCalledWith(
            expect.objectContaining({ scope: "managed", pageSize: 1 }),
        )
    })

    it("hides the managed metric without work_item:manage", async () => {
        const dashboard = await fetchWorkspaceDashboard(baseQuery, profile)
        const managed = dashboard.metrics.find(
            (metric) => metric.key === "managed",
        )

        expect(managed).toMatchObject({ visible: false, count: 0 })
        expect(mocks.listWorkItems).toHaveBeenCalledWith(
            expect.objectContaining({ scope: "mine" }),
        )
        expect(mocks.listWorkItems).not.toHaveBeenCalledWith(
            expect.objectContaining({ scope: "managed" }),
        )
    })

    it("lists managed work items when the managed view is selected", async () => {
        mocks.listWorkItems.mockResolvedValue({
            items: [managedItem],
            total: 1,
        })

        const dashboard = await fetchWorkspaceDashboard(
            { ...baseQuery, view: "managed" },
            manager,
        )

        expect(dashboard.items).toHaveLength(1)
        expect(dashboard.items[0]).toMatchObject({
            workItemId: "wi-managed",
            ownerRole: "role-procurement",
            stableNumber: "XS1",
        })
        expect(mocks.listWorkItems).toHaveBeenCalledWith(
            expect.objectContaining({ scope: "managed", pageSize: 50 }),
        )
        expect(
            dashboard.metrics.find((metric) => metric.key === "managed"),
        ).toMatchObject({ visible: true, count: 1 })
    })

    it("treats managed 403 as no management scope instead of failing the page", async () => {
        const { createApiError } = await import("@/lib/api")
        mocks.listWorkItems.mockImplementation(
            async (input: { scope?: string }) => {
                if (input.scope === "managed") {
                    throw createApiError({
                        kind: "Http",
                        status: 403,
                        message: "当前账号没有任务责任管理范围",
                    })
                }
                return { items: [], total: 0 }
            },
        )

        const dashboard = await fetchWorkspaceDashboard(
            { ...baseQuery, view: "managed" },
            manager,
        )

        expect(dashboard.items).toEqual([])
        expect(
            dashboard.metrics.find((metric) => metric.key === "managed"),
        ).toMatchObject({ visible: false, count: 0 })
        expect(mocks.listWorkItems).toHaveBeenCalledWith(
            expect.objectContaining({ scope: "mine" }),
        )
    })
})
