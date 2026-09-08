import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import type { TodayWorkspaceView } from "@/features/workspace/types"

const home = vi.hoisted(() => ({
    current: {} as Record<string, unknown>,
}))

vi.mock("@/features/workspace/hooks/use-workspace-home", () => ({
    useWorkspaceHome: () => home.current,
}))

vi.mock("@/features/workspace/components/workspace-task-detail", async () => {
    const { WorkspacePaneActions } =
        await import("@/features/workspace/components/workspace-pane-actions")
    return {
        WorkspaceTaskDetail: () => (
            <div className="flex items-center justify-between">
                <h1>供给分配</h1>
                <WorkspacePaneActions />
            </div>
        ),
    }
})

import { WorkspaceHomePage } from "./workspace-home-page"

afterEach(cleanup)

function emptyAllowedView(
    overrides: Partial<TodayWorkspaceView> = {},
): TodayWorkspaceView {
    return {
        access: "allowed",
        viewer: {
            userId: "u1",
            displayName: "测试",
            activeRoleLabel: "采购",
            timezone: "Asia/Shanghai",
        },
        freshness: {
            workItemsUpdatedAt: "2026-08-30T02:00:00.000Z",
            statsUpdatedAt: "2026-08-30T02:00:00.000Z",
            statsState: "fresh",
            projectionUpdatedAt: "2026-08-30T02:00:00.000Z",
            projectionState: "fresh",
        },
        metrics: [
            {
                key: "inbox",
                label: "待我处理",
                count: 0,
                visible: true,
                tone: "neutral",
            },
            {
                key: "overdue",
                label: "已超期",
                count: 0,
                visible: true,
                tone: "warning",
            },
            {
                key: "blocked",
                label: "受阻",
                count: 0,
                visible: true,
                tone: "destructive",
            },
            {
                key: "started",
                label: "我发起的",
                count: 0,
                visible: true,
                tone: "neutral",
            },
        ],
        familyCounts: {
            approval: 0,
            procurement: 0,
            fulfillment: 0,
            finance: 0,
            exception: 0,
        },
        items: [],
        total: 0,
        warnings: [],
        recent: [],
        ...overrides,
    }
}

function stubHome(
    view: TodayWorkspaceView,
    extras: Record<string, unknown> = {},
) {
    home.current = {
        urlState: { view: "inbox", sort: "priority_due" },
        view,
        accountProfileQuery: {
            isPending: false,
            isError: false,
            isFetching: false,
            data: { permissions: [] },
        },
        dashboardQuery: {
            isPending: false,
            isError: false,
            isFetching: false,
            data: view,
        },
        refreshing: false,
        activeMetric: "inbox",
        hasActiveFilter: false,
        searchDraft: "",
        setSearchDraft: vi.fn(),
        narrowDetailOpen: false,
        setNarrowDetailOpen: vi.fn(),
        setNarrowDetailSettledOpen: vi.fn(),
        completionAnnouncement: { sequence: 0, text: "" },
        selected: undefined,
        onMetricClick: vi.fn(),
        clearFilters: vi.fn(),
        onSelectTask: vi.fn(),
        applyDecisionAfter: vi.fn(),
        onFamilyChange: vi.fn(),
        onSortChange: vi.fn(),
        applySearch: vi.fn(),
        clearSearch: vi.fn(),
        refresh: vi.fn(),
        ...extras,
    }
}

test("无待办时显示单一空态，筛选仍在列表上方", () => {
    stubHome(emptyAllowedView())
    render(<WorkspaceHomePage />)
    expect(screen.getByText("当前没有待处理事项")).toBeTruthy()
    expect(document.querySelector('[data-slot="workspace-detail"]')).toBeNull()
    const queue = document.querySelector('[data-slot="workspace-queue"]')
    expect(
        queue?.contains(screen.getByRole("group", { name: "任务类型" })),
    ).toBe(false)
})

test("筛选无结果时保留条件且可以恢复全部待办", () => {
    const clearFilters = vi.fn()
    stubHome(emptyAllowedView(), {
        urlState: { view: "inbox", sort: "priority_due", family: "approval" },
        hasActiveFilter: true,
        clearFilters,
    })
    render(<WorkspaceHomePage />)
    expect(screen.getByRole("button", { name: "任务类型：审批" })).toBeTruthy()
    fireEvent.click(document.getElementById("workspace-home-clear-filters")!)
    expect(clearFilters).toHaveBeenCalledTimes(1)
    expect(document.querySelector('[data-slot="workspace-detail"]')).toBeNull()
})

test("我发起的审批显示搜索且不展示任务类型", () => {
    const applySearch = vi.fn()
    stubHome(emptyAllowedView(), {
        urlState: { view: "started", sort: "priority_due" },
        activeMetric: "started",
        applySearch,
    })
    render(<WorkspaceHomePage />)

    const search = screen.getByLabelText("搜索我发起的审批")
    expect(search).toBeTruthy()
    expect(screen.queryByRole("group", { name: "任务类型" })).toBeNull()
    expect(screen.queryByLabelText("排序：超期与优先级")).toBeNull()
    expect(screen.getByText("还没有我发起的审批")).toBeTruthy()
    fireEvent.submit(search.closest("form") as HTMLFormElement)
    expect(applySearch).toHaveBeenCalledTimes(1)
})

test("我发起的审批搜索无结果时可以清除关键词", () => {
    const clearSearch = vi.fn()
    stubHome(emptyAllowedView(), {
        urlState: { view: "started", sort: "priority_due", query: "SO-1" },
        activeMetric: "started",
        searchDraft: "SO-1",
        clearSearch,
    })
    render(<WorkspaceHomePage />)

    expect(screen.getByText("没有匹配的审批")).toBeTruthy()
    const action = screen.getByRole("button", { name: "清除搜索" })
    expect(action).toBeTruthy()
    action.click()
    expect(clearSearch).toHaveBeenCalledTimes(1)
})

test("右侧全屏会收起左列队列，再次点击恢复", () => {
    stubHome(emptyAllowedView({ total: 1 }), {
        selected: {
            workItemId: "wi-1",
            objectTitle: "销售单 XS1",
        },
    })
    render(<WorkspaceHomePage />)

    const queue = document.querySelector('[data-slot="workspace-queue"]')
    expect(queue?.className.includes("hidden")).toBe(false)

    fireEvent.click(screen.getByRole("button", { name: "全屏处理" }))
    expect(queue?.className.includes("hidden")).toBe(true)
    expect(screen.getByRole("button", { name: "退出全屏" })).toBeTruthy()

    fireEvent.click(screen.getByRole("button", { name: "退出全屏" }))
    expect(queue?.className.includes("hidden")).toBe(false)
})

test("关闭详情后列表恢复全宽，同一任务可以再次打开", () => {
    const item = {
        workItemId: "wi-1",
        objectTitle: "采购单",
        stableNumber: "PO-1",
        workItemTypeLabel: "采购审批",
        workItemType: "DOCUMENT_APPROVAL",
        family: "approval",
        counterpartyName: "测试供应商",
        dueAtLabel: "今天",
        dueBucket: "today",
        processingState: "READY",
    } as TodayWorkspaceView["items"][number]
    const onSelectTask = vi.fn()
    stubHome(emptyAllowedView({ items: [item], total: 1 }), {
        selected: item,
        onSelectTask,
    })
    render(<WorkspaceHomePage />)
    fireEvent.click(screen.getByRole("button", { name: "关闭详情" }))
    expect(document.querySelector('[data-slot="workspace-detail"]')).toBeNull()
    const row = screen.getByRole("button", { name: "采购审批 测试供应商 PO-1" })
    expect(row.getAttribute("aria-current")).toBeNull()
    fireEvent.click(row)
    expect(onSelectTask).toHaveBeenCalledWith(item)
    expect(
        document.querySelector('[data-slot="workspace-detail"]'),
    ).toBeTruthy()
    expect(row.getAttribute("aria-current")).toBe("true")
})
