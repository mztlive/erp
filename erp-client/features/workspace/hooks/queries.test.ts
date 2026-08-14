import { beforeEach, describe, expect, it, vi } from "vitest"
import { waitFor } from "@testing-library/react"

import type { AccountProfile } from "@/features/auth/api"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { fetchWorkspaceDashboard } from "@/features/workspace/api/dashboard"
import type {
    TodayWorkspaceQuery,
    TodayWorkspaceView,
} from "@/features/workspace/types"
import { useWorkspaceDashboardQuery } from "./queries"

vi.mock("@/features/workspace/api/dashboard", () => ({
    fetchWorkspaceDashboard: vi.fn(),
}))

const mockedFetch = vi.mocked(fetchWorkspaceDashboard)

const queryFixture: TodayWorkspaceQuery = {
    scope: "mine",
    due: "today",
    timezone: "Asia/Shanghai",
}

const profileFixture: AccountProfile = {
    userid: "u1",
    account: "admin01",
    name: "管理员",
    subject: "subj",
    role_ids: ["r2", "r1"],
    permissions: ["work:*", "*:*"],
    account_kind: "admin",
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

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useWorkspaceDashboardQuery", () => {
    it("stays disabled and never fetches until the profile is ready", () => {
        const { result } = renderHookWithProviders(() =>
            useWorkspaceDashboardQuery(queryFixture),
        )

        expect(result.current.fetchStatus).toBe("idle")
        expect(result.current.data).toBeUndefined()
        expect(mockedFetch).not.toHaveBeenCalled()
    })

    it("passes the query input and profile to the api and caches under a stable key", async () => {
        mockedFetch.mockResolvedValue(viewFixture)
        const queryClient = createFreshQueryClient()

        renderHookWithProviders(
            () => useWorkspaceDashboardQuery(queryFixture, profileFixture),
            { queryClient },
        )

        await waitFor(() =>
            expect(mockedFetch).toHaveBeenCalledWith(
                queryFixture,
                profileFixture,
            ),
        )
        // queryKey: profile 的 role_ids / permissions 需排序，保证键稳定。
        expect(
            queryClient.getQueryData([
                "workspace-home",
                "dashboard",
                queryFixture,
                "u1",
                ["r1", "r2"],
                ["*:*", "work:*"],
            ]),
        ).toEqual(viewFixture)
    })

    it("keeps previous data visible while the next filter is loading", async () => {
        mockedFetch.mockResolvedValue(viewFixture)
        const queryClient = createFreshQueryClient()
        let query = queryFixture

        const { result, rerender } = renderHookWithProviders(
            () => useWorkspaceDashboardQuery(query, profileFixture),
            { queryClient },
        )

        await waitFor(() => expect(result.current.data).toEqual(viewFixture))

        query = { ...queryFixture, due: "overdue" }
        rerender()

        expect(result.current.isPlaceholderData).toBe(true)
        expect(result.current.data).toEqual(viewFixture)
    })

    it("surfaces request errors on the query state", async () => {
        mockedFetch.mockRejectedValue(new Error("dashboard down"))
        const { result } = renderHookWithProviders(() =>
            useWorkspaceDashboardQuery(queryFixture, profileFixture),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})
