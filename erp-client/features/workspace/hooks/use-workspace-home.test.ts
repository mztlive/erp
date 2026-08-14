import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, waitFor } from "@testing-library/react"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { renderHookWithProviders } from "@/features/test-utils"
import { writeW02FocusId } from "@/features/unified-task-queue/queue-url"
import { useWorkspaceDashboardQuery } from "@/features/workspace/hooks/queries"
import type {
    TodayWorkspaceView,
    WorkspaceWorkItem,
} from "@/features/workspace/types"
import { useSearchParams } from "next/navigation"
import { useWorkspaceHome } from "./use-workspace-home"

const { mockReplace } = vi.hoisted(() => ({
    mockReplace: vi.fn(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: vi.fn(),
        replace: mockReplace,
        back: vi.fn(),
    }),
    useSearchParams: vi.fn(() => new URLSearchParams()),
    usePathname: () => "/workspace",
    useParams: () => ({}),
}))

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: vi.fn(),
}))

vi.mock("@/features/workspace/hooks/queries", () => ({
    useWorkspaceDashboardQuery: vi.fn(),
}))

vi.mock("@/features/unified-task-queue/queue-url", () => ({
    writeW02FocusId: vi.fn(),
}))

const FOCUS_SESSION_KEY = "workspace-home.focus"

const profileFixture = {
    userid: "u1",
    account: "admin01",
    name: "管理员",
    subject: "subj",
    role_ids: ["r1"],
    permissions: ["*:*"],
    account_kind: "admin" as const,
}

const viewFixture: TodayWorkspaceView = {
    access: "allowed",
    viewer: {
        userId: "u1",
        displayName: "管理员",
        activeRoleLabel: "管理员",
        timezone: "Asia/Shanghai",
    },
    freshness: {
        workItemsUpdatedAt: "",
        projectionUpdatedAt: "",
        projectionState: "fresh",
    },
    metrics: [],
    groups: [],
    warnings: [],
    recent: [],
    canOpenTaskQueue: true,
    temporaryPreviewLimitFallback: 5,
}

function workItemFixture(
    overrides: Partial<WorkspaceWorkItem> = {},
): WorkspaceWorkItem {
    return {
        workItemId: "wi-1",
        taskVersion: "v1",
        workItemType: "BUSINESS_EXCEPTION",
        workItemTypeLabel: "业务异常",
        businessObjectType: "SALES",
        businessObjectId: "N-1",
        subjectVersion: "sv-1",
        stableNumber: "N-1",
        objectTitle: "销售单 N-1",
        counterpartyName: "客户A",
        status: "OPEN",
        statusLabel: "待处理",
        statusTone: "info",
        processingState: "READY",
        assignmentMode: "DIRECT",
        priority: 3,
        createdAt: "",
        dueAt: "",
        ownerRoleLabel: "销售",
        ownerOrganizationLabel: "总部",
        reasonLabel: "",
        impactSummary: "",
        allowedActions: ["PROCESS", "VIEW"],
        actionBlockers: [],
        destinationWorkspaceId: "W02",
        handlerKey: "w02.process",
        enteredAtLabel: "",
        dueAtLabel: "",
        dueBucket: "later",
        family: "exception",
        ...overrides,
    }
}

function mockAccountProfileQuery(
    overrides: Partial<ReturnType<typeof useAccountProfileQuery>> = {},
) {
    const value = {
        data: profileFixture,
        isPending: false,
        isFetching: false,
        isError: false,
        error: null,
        refetch: vi.fn().mockResolvedValue({ isSuccess: true }),
        ...overrides,
    }
    vi.mocked(useAccountProfileQuery).mockReturnValue(
        value as ReturnType<typeof useAccountProfileQuery>,
    )
    return value
}

function mockDashboardQuery(
    overrides: Partial<ReturnType<typeof useWorkspaceDashboardQuery>> = {},
) {
    const value = {
        data: undefined,
        isPending: true,
        isFetching: false,
        isError: false,
        error: null,
        refetch: vi.fn().mockResolvedValue({ isSuccess: true }),
        ...overrides,
    }
    vi.mocked(useWorkspaceDashboardQuery).mockReturnValue(
        value as ReturnType<typeof useWorkspaceDashboardQuery>,
    )
    return value
}

function mockSearchParams(raw: string) {
    vi.mocked(useSearchParams).mockReturnValue(
        new URLSearchParams(raw) as unknown as ReturnType<
            typeof useSearchParams
        >,
    )
}

let scrollIntoView: ReturnType<typeof vi.fn<(arg?: boolean | ScrollIntoViewOptions) => void>>

beforeEach(() => {
    vi.clearAllMocks()
    sessionStorage.clear()
    document.body.innerHTML = ""
    scrollIntoView = vi.fn<(arg?: boolean | ScrollIntoViewOptions) => void>()
    Element.prototype.scrollIntoView = scrollIntoView
    mockSearchParams("")
})

describe("useWorkspaceHome", () => {
    it("derives the default url state, metric and filter flag from empty params", () => {
        mockAccountProfileQuery()
        mockDashboardQuery()

        const { result } = renderHookWithProviders(() => useWorkspaceHome())

        expect(result.current.urlState).toEqual({ scope: "mine" })
        expect(result.current.activeMetric).toBe("mine")
        expect(result.current.hasActiveFilter).toBe(false)
    })

    it("parses scope and due from the url params", () => {
        mockSearchParams("scope=team&due=today")
        mockAccountProfileQuery()
        mockDashboardQuery()

        const { result } = renderHookWithProviders(() => useWorkspaceHome())

        expect(result.current.urlState).toEqual({
            scope: "team",
            due: "today",
        })
        expect(result.current.activeMetric).toBe("due_today")
        expect(result.current.hasActiveFilter).toBe(true)
    })

    it("feeds the dashboard hook a today query with the viewer timezone", () => {
        mockSearchParams("scope=team&family=exception")
        mockAccountProfileQuery()
        mockDashboardQuery()

        renderHookWithProviders(() => useWorkspaceHome())

        expect(useWorkspaceDashboardQuery).toHaveBeenCalledWith(
            {
                scope: "team",
                family: "exception",
                timezone: "Asia/Shanghai",
            },
            profileFixture,
        )
    })

    it("replaces the url when the scope changes and clears the stored focus", () => {
        sessionStorage.setItem(FOCUS_SESSION_KEY, "N-9")
        mockAccountProfileQuery()
        mockDashboardQuery()

        const { result } = renderHookWithProviders(() => useWorkspaceHome())

        act(() => result.current.onScopeChange("team"))

        expect(mockReplace).toHaveBeenCalledWith("/workspace?scope=team", {
            scroll: false,
        })
        expect(sessionStorage.getItem(FOCUS_SESSION_KEY)).toBeNull()
        expect(result.current.focusedStableNumber).toBeNull()
    })

    it("keeps the url untouched when the same scope is chosen again", () => {
        mockAccountProfileQuery()
        mockDashboardQuery()

        const { result } = renderHookWithProviders(() => useWorkspaceHome())

        act(() => result.current.onScopeChange("mine"))

        expect(mockReplace).not.toHaveBeenCalled()
    })

    it("rewrites filters from the metric key, keeping the scope", () => {
        mockSearchParams("scope=team")
        mockAccountProfileQuery()
        mockDashboardQuery()

        const { result } = renderHookWithProviders(() => useWorkspaceHome())

        act(() => result.current.onMetricClick("exception"))

        expect(mockReplace).toHaveBeenCalledWith(
            "/workspace?scope=team&family=exception",
            { scroll: false },
        )
    })

    it("clears filters back to the default scope url", () => {
        mockSearchParams("scope=team&due=overdue")
        sessionStorage.setItem(FOCUS_SESSION_KEY, "N-9")
        mockAccountProfileQuery()
        mockDashboardQuery()

        const { result } = renderHookWithProviders(() => useWorkspaceHome())

        act(() => result.current.clearFilters())

        expect(mockReplace).toHaveBeenCalledWith("/workspace", {
            scroll: false,
        })
        expect(sessionStorage.getItem(FOCUS_SESSION_KEY)).toBeNull()
    })

    it("stores the focus and preloads W02 focus for PROCESS intents", () => {
        mockAccountProfileQuery()
        mockDashboardQuery()

        const { result } = renderHookWithProviders(() => useWorkspaceHome())

        act(() =>
            result.current.onOpenTask(workItemFixture(), "PROCESS"),
        )

        expect(sessionStorage.getItem(FOCUS_SESSION_KEY)).toBe("N-1")
        expect(writeW02FocusId).toHaveBeenCalledWith("wi-1")
    })

    it("skips the W02 focus preload for W18 targets and VIEW intents", () => {
        mockAccountProfileQuery()
        mockDashboardQuery()

        const { result } = renderHookWithProviders(() => useWorkspaceHome())

        act(() =>
            result.current.onOpenTask(
                workItemFixture({ destinationWorkspaceId: "W18" }),
                "PROCESS",
            ),
        )
        act(() => result.current.onOpenTask(workItemFixture(), "VIEW"))

        expect(writeW02FocusId).not.toHaveBeenCalled()
        expect(sessionStorage.getItem(FOCUS_SESSION_KEY)).toBe("N-1")
    })

    it("restores the stored focus once when the dashboard view arrives", () => {
        const el = document.createElement("div")
        el.id = "work-item-N-100"
        // 聚焦行渲染时会带上 tabIndex=-1（页面以此让行可聚焦）。
        el.tabIndex = -1
        document.body.appendChild(el)
        sessionStorage.setItem(FOCUS_SESSION_KEY, "N-100")
        mockAccountProfileQuery()
        mockDashboardQuery({
            data: viewFixture,
            isPending: false,
        })

        const { result } = renderHookWithProviders(() => useWorkspaceHome())

        expect(result.current.focusedStableNumber).toBe("N-100")
        expect(sessionStorage.getItem(FOCUS_SESSION_KEY)).toBeNull()
        expect(scrollIntoView).toHaveBeenCalledWith({
            block: "nearest",
            behavior: "smooth",
        })
        expect(document.activeElement).toBe(el)
    })

    it("refetches the dashboard after a successful profile refetch", async () => {
        mockAccountProfileQuery()
        const dashboard = mockDashboardQuery({
            data: viewFixture,
            isPending: false,
        })

        const { result } = renderHookWithProviders(() => useWorkspaceHome())

        act(() => {
            result.current.refresh()
        })

        await waitFor(() => expect(dashboard.refetch).toHaveBeenCalled())
    })

    it("reports refreshing only while fetching with data present", () => {
        mockAccountProfileQuery({ isFetching: true })
        mockDashboardQuery({ data: viewFixture, isPending: false })
        const { result } = renderHookWithProviders(() => useWorkspaceHome())
        expect(result.current.refreshing).toBe(true)
    })

    it("does not report refreshing while the first load is pending", () => {
        mockAccountProfileQuery({ isFetching: true })
        mockDashboardQuery({ data: undefined, isPending: true, isFetching: true })
        const { result } = renderHookWithProviders(() => useWorkspaceHome())
        expect(result.current.refreshing).toBe(false)
    })
})
