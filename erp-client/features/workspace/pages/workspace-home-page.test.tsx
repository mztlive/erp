import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import type { WorkspaceWorkItem } from "@/features/workspace/types"

import { WorkspaceHomePage } from "./workspace-home-page"

const home = vi.hoisted(() => ({
    useWorkspaceHome: vi.fn(),
}))

vi.mock("@/features/workspace/hooks/use-workspace-home", () => home)
vi.mock("@/features/approval-workflow/components/approval-action-bar", () => ({
    ApprovalActionBar: ({
        allowedActions,
    }: {
        allowedActions: readonly string[]
    }) => (
        <div>
            {allowedActions.includes("APPROVE") ? (
                <button type="button">通过</button>
            ) : null}
            {allowedActions.includes("REJECT") ? (
                <button type="button">驳回</button>
            ) : null}
        </div>
    ),
}))
vi.mock("@/features/approval-workflow/queries", () => ({
    useRecoveryOptionsQuery: () => ({ data: { actions: [] } }),
}))
vi.mock("@/features/workspace/hooks/use-workspace-document-facts", () => ({
    useWorkspaceDocumentFacts: () => ({ facts: null, isPending: false }),
}))
vi.mock("next/navigation", () => ({
    useRouter: () => ({ replace: vi.fn(), push: vi.fn() }),
    usePathname: () => "/workspace",
    useSearchParams: () => new URLSearchParams(),
}))

function workItemFixture(
    overrides: Partial<WorkspaceWorkItem> = {},
): WorkspaceWorkItem {
    return {
        workItemId: "wi-1",
        taskVersion: "1",
        workItemType: "SALES_APPROVAL",
        workItemTypeLabel: "销售单审批",
        businessObjectType: "SalesOrder",
        businessObjectId: "SO-1",
        subjectVersion: "1",
        stableNumber: "SO-1",
        objectTitle: "SO-2026-0031",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "info",
        processingState: "READY",
        priority: 1,
        createdAt: "",
        ownerRoleLabel: "销售",
        ownerOrganizationLabel: "华东",
        ownerUserLabel: "由你处理",
        reasonLabel: "",
        impactSummary: "",
        nextActionHint: "进入对应页面后提交处理结论。",
        allowedActions: ["APPROVE", "REJECT"],
        actionBlockers: [],
        destinationWorkspaceId: "W05",
        handlerKey: "low_margin_manager",
        enteredAtLabel: "",
        dueAtLabel: "",
        dueBucket: "later",
        family: "approval",
        approvalProcessInstanceId: "inst-1",
        ...overrides,
    }
}

function homeState(overrides: Record<string, unknown> = {}) {
    const item = workItemFixture()
    return {
        urlState: { view: "inbox", sort: "priority_due" },
        view: {
            access: "allowed",
            viewer: {
                userId: "u1",
                displayName: "周航",
                activeRoleLabel: "销售",
                timezone: "Asia/Shanghai",
            },
            freshness: {
                workItemsUpdatedAt: "",
                statsUpdatedAt: "",
                statsState: "fresh",
                projectionUpdatedAt: "",
                projectionState: "fresh",
            },
            metrics: [
                {
                    key: "inbox",
                    label: "待我处理",
                    count: 2,
                    visible: true,
                    tone: "info",
                },
                {
                    key: "overdue",
                    label: "已超期",
                    count: 0,
                    visible: true,
                    tone: "destructive",
                },
            ],
            items: [item],
            total: 2,
            warnings: [],
            recent: [],
        },
        accountProfileQuery: { isPending: false, isError: false },
        dashboardQuery: { isPending: false, isError: false },
        refreshing: false,
        activeMetric: "inbox",
        hasActiveFilter: false,
        searchDraft: "",
        setSearchDraft: vi.fn(),
        narrowDetailOpen: false,
        setNarrowDetailOpen: vi.fn(),
        selected: item,
        onMetricClick: vi.fn(),
        clearFilters: vi.fn(),
        onSelectTask: vi.fn(),
        selectNextAfter: vi.fn(),
        applyDecisionAfter: vi.fn(),
        onFamilyChange: vi.fn(),
        onSortChange: vi.fn(),
        applySearch: vi.fn(),
        refresh: vi.fn(),
        ...overrides,
    }
}

describe("WorkspaceHomePage", () => {
    afterEach(() => {
        cleanup()
    })

    it("has no team partition and no second task-queue link", () => {
        home.useWorkspaceHome.mockReturnValue(homeState())

        render(<WorkspaceHomePage />)
        expect(screen.getByRole("heading", { name: "我的工作台" })).toBeTruthy()
        expect(screen.getAllByText("待我处理").length).toBeGreaterThan(0)
        expect(screen.queryByText("团队待处理")).toBeNull()
        expect(screen.queryByText("查看全部待办")).toBeNull()
        expect(screen.queryByRole("link", { name: /统一待办/ })).toBeNull()
        expect(screen.getByRole("button", { name: "通过" })).toBeTruthy()
        expect(screen.getByRole("button", { name: "驳回" })).toBeTruthy()
    })

    it("does not render metric cards, greeting, or dual freshness", () => {
        home.useWorkspaceHome.mockReturnValue(homeState())

        render(<WorkspaceHomePage />)
        expect(screen.queryByText(/早上好/)).toBeNull()
        expect(screen.queryByText("当前授权范围")).toBeNull()
        expect(screen.queryByText("工作台汇总")).toBeNull()
        expect(document.querySelector('[data-slot="metric-strip"]')).toBeNull()
        expect(screen.getByLabelText("待办筛选")).toBeTruthy()
        expect(screen.getByLabelText("任务类型")).toBeTruthy()
        expect(screen.queryByRole("button", { name: "搜索" })).toBeNull()
        expect(screen.getByLabelText("搜索待办")).toBeTruthy()
        expect(document.querySelector("[data-slot=page-scaffold]")).toBeTruthy()
    })

    it("shows a single empty state when there are no tasks", () => {
        home.useWorkspaceHome.mockReturnValue(
            homeState({
                view: {
                    ...homeState().view,
                    items: [],
                    total: 0,
                    metrics: [
                        {
                            key: "inbox",
                            label: "待我处理",
                            count: 0,
                            visible: true,
                            tone: "info",
                        },
                    ],
                },
                selected: undefined,
            }),
        )

        render(<WorkspaceHomePage />)
        expect(screen.getAllByText("当前没有待处理事项")).toHaveLength(1)
        expect(screen.queryByText("当前没有待处理事项")).toBeTruthy()
        expect(screen.queryByLabelText("待办列表")).toBeNull()
    })
})
