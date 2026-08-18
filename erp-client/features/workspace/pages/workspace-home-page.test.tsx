import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

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
vi.mock("next/navigation", () => ({
    useRouter: () => ({ replace: vi.fn(), push: vi.fn() }),
    usePathname: () => "/workspace",
    useSearchParams: () => new URLSearchParams(),
}))

describe("WorkspaceHomePage", () => {
    it("has no team partition and no second task-queue link", () => {
        home.useWorkspaceHome.mockReturnValue({
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
                ],
                items: [
                    {
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
                        allowedActions: ["APPROVE", "REJECT"],
                        actionBlockers: [],
                        destinationWorkspaceId: "W05",
                        handlerKey: "low_margin_manager",
                        enteredAtLabel: "",
                        dueAtLabel: "",
                        dueBucket: "later",
                        family: "approval",
                        approvalProcessInstanceId: "inst-1",
                    },
                ],
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
            selected: {
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
                allowedActions: ["APPROVE", "REJECT"],
                actionBlockers: [],
                destinationWorkspaceId: "W05",
                handlerKey: "low_margin_manager",
                enteredAtLabel: "",
                dueAtLabel: "",
                dueBucket: "later",
                family: "approval",
                approvalProcessInstanceId: "inst-1",
            },
            onMetricClick: vi.fn(),
            clearFilters: vi.fn(),
            onSelectTask: vi.fn(),
            selectNextAfter: vi.fn(),
            onFamilyChange: vi.fn(),
            onSortChange: vi.fn(),
            applySearch: vi.fn(),
            refresh: vi.fn(),
        })

        render(<WorkspaceHomePage />)
        expect(screen.getAllByText("待我处理").length).toBeGreaterThan(0)
        expect(screen.queryByText("团队待处理")).toBeNull()
        expect(screen.queryByText("查看全部待办")).toBeNull()
        expect(screen.queryByRole("link", { name: /统一待办/ })).toBeNull()
        expect(screen.getByRole("button", { name: "通过" })).toBeTruthy()
        expect(screen.getByRole("button", { name: "驳回" })).toBeTruthy()
    })
})
