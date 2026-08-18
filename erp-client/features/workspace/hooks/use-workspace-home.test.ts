import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, waitFor } from "@testing-library/react"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { renderHookWithProviders } from "@/features/test-utils"
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
        statsUpdatedAt: "",
        statsState: "fresh",
        projectionUpdatedAt: "",
        projectionState: "fresh",
    },
    metrics: [],
    items: [],
    total: 0,
    warnings: [],
    recent: [],
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
        priority: 3,
        createdAt: "",
        ownerRoleLabel: "销售",
        ownerOrganizationLabel: "总部",
        ownerUserLabel: "张三",
        reasonLabel: "",
        impactSummary: "",
        allowedActions: ["PROCESS", "VIEW"],
        actionBlockers: [],
        destinationWorkspaceId: "W01",
        handlerKey: "business_exception",
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

beforeEach(() => {
    vi.clearAllMocks()
    mockSearchParams("")
})

describe("useWorkspaceHome", () => {
    it("defaults to inbox and has no team partition", () => {
        mockAccountProfileQuery()
        mockDashboardQuery()
        const { result } = renderHookWithProviders(() => useWorkspaceHome())
        expect(result.current.urlState.view).toBe("inbox")
        expect(result.current.activeMetric).toBe("inbox")
        expect(result.current.hasActiveFilter).toBe(false)
        expect(result.current).not.toHaveProperty("onScopeChange")
    })

    it("treats metric clicks as same-page filters", () => {
        mockAccountProfileQuery()
        mockDashboardQuery()
        const { result } = renderHookWithProviders(() => useWorkspaceHome())
        act(() => result.current.onMetricClick("overdue"))
        const href = String(mockReplace.mock.calls[0]?.[0])
        expect(href.startsWith("/workspace")).toBe(true)
        expect(href).toContain("due=overdue")
        expect(href).not.toContain("scope=team")
    })

    it("selects the next item after an approval decision", () => {
        mockAccountProfileQuery()
        mockDashboardQuery({
            data: {
                ...viewFixture,
                items: [
                    workItemFixture({ workItemId: "wi-1" }),
                    workItemFixture({
                        workItemId: "wi-2",
                        stableNumber: "N-2",
                    }),
                ],
                total: 2,
            },
            isPending: false,
        })
        mockSearchParams("currentWorkItemId=wi-1")
        const { result } = renderHookWithProviders(() => useWorkspaceHome())
        act(() => result.current.selectNextAfter("wi-1"))
        const href = String(mockReplace.mock.calls[0]?.[0])
        expect(href).toContain("currentWorkItemId=wi-2")
        expect(href).not.toContain("scope=team")
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
})
