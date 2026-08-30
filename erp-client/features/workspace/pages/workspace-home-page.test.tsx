import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test, vi } from "vitest"

import type { TodayWorkspaceView } from "@/features/workspace/types"

const home = vi.hoisted(() => ({
    current: {} as Record<string, unknown>,
}))

vi.mock("@/features/workspace/hooks/use-workspace-home", () => ({
    useWorkspaceHome: () => home.current,
}))

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
        refresh: vi.fn(),
        ...extras,
    }
}

test("队列为空时仍渲染左右分栏，空态落在队列列", () => {
    stubHome(emptyAllowedView())
    render(<WorkspaceHomePage />)

    const queue = document.querySelector('[data-slot="workspace-queue"]')
    const detail = document.querySelector('[data-slot="workspace-detail"]')
    expect(queue).toBeTruthy()
    expect(detail).toBeTruthy()
    expect(queue?.className).toContain("lg:w-80")
    expect(detail?.className).toContain("flex-1")

    expect(screen.getByRole("group", { name: "任务类型" })).toBeTruthy()
    expect(queue?.contains(screen.getByText("当前没有待处理事项"))).toBe(true)
    expect(detail?.contains(screen.getByText("在此处理任务"))).toBe(true)
})

test("筛选无结果时清空动作留在左列，右列作业面仍在", () => {
    stubHome(emptyAllowedView(), {
        urlState: { view: "inbox", sort: "priority_due", family: "approval" },
        hasActiveFilter: true,
    })
    render(<WorkspaceHomePage />)

    expect(screen.getByRole("button", { name: "回到待我处理" })).toBeTruthy()
    const queue = document.querySelector('[data-slot="workspace-queue"]')
    const detail = document.querySelector('[data-slot="workspace-detail"]')
    expect(
        queue?.contains(screen.getByRole("button", { name: "回到待我处理" })),
    ).toBe(true)
    expect(detail).toBeTruthy()
    expect(screen.getByText("在此处理任务")).toBeTruthy()
})
