import { act, renderHook, waitFor } from "@testing-library/react"
import { QueryClientProvider } from "@tanstack/react-query"
import * as React from "react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { useAccessDetailPanels } from "./use-access-detail-panels"
import { useAccessListControls } from "./use-access-list-controls"
import { useAccessUrlState } from "./use-access-url-state"

const wrapperWithClient = () => {
    const client = createFreshQueryClient()
    return ({ children }: { children: React.ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    )
}

let currentSearchParams = ""
let mockReplace: ReturnType<typeof vi.fn>
let mockPush: ReturnType<typeof vi.fn>

vi.mock("next/navigation", () => ({
    useRouter: () => ({ push: mockPush, replace: mockReplace, back: vi.fn() }),
    useSearchParams: () => new URLSearchParams(currentSearchParams),
    usePathname: () => "/system/access-audit",
    useParams: () => ({}),
}))

beforeEach(() => {
    currentSearchParams = ""
    mockPush = vi.fn()
    mockReplace = vi.fn()
    vi.clearAllMocks()
})

describe("useAccessUrlState", () => {
    it("parses all list and detail URL params and keeps the view fallback", () => {
        currentSearchParams =
            "view=audit&q=abc&status=enabled&org=o1&risk=HIGH_PRIVILEGE" +
            "&subjectType=USER&subjectId=u1&eventId=ae_1&from=2026-01-01" +
            "&to=2026-01-02&actorId=a1&action=QUERY_AUDIT&objectType=audit" +
            "&objectId=o1&result=SUCCESS&traceId=tr_1&workItemId=wi_1"
        const { result } = renderHookWithProviders(() => useAccessUrlState())

        expect(result.current.view).toBe("audit")
        expect(result.current.qParam).toBe("abc")
        expect(result.current.status).toBe("enabled")
        expect(result.current.org).toBe("o1")
        expect(result.current.risk).toBe("HIGH_PRIVILEGE")
        expect(result.current.subjectTypeParam).toBe("USER")
        expect(result.current.subjectIdParam).toBe("u1")
        expect(result.current.eventIdParam).toBe("ae_1")
        expect(result.current.fromParam).toBe("2026-01-01")
        expect(result.current.toParam).toBe("2026-01-02")
        expect(result.current.actorId).toBe("a1")
        expect(result.current.action).toBe("QUERY_AUDIT")
        expect(result.current.objectType).toBe("audit")
        expect(result.current.objectId).toBe("o1")
        expect(result.current.resultFilter).toBe("SUCCESS")
        expect(result.current.traceId).toBe("tr_1")
        expect(result.current.rejectedWorkItemId).toBe("wi_1")
    })

    it("treats missing params as undefined and defaults the view to roles", () => {
        currentSearchParams = ""
        const { result } = renderHookWithProviders(() => useAccessUrlState())
        expect(result.current.view).toBe("roles")
        expect(result.current.qParam).toBe("")
        expect(result.current.status).toBeUndefined()
        expect(result.current.subjectIdParam).toBeUndefined()
    })

    it("patches the URL keeping the view param and clearing null values", () => {
        currentSearchParams = "view=roles&q=old"
        const { result } = renderHookWithProviders(() => useAccessUrlState())
        act(() => {
            result.current.patchUrl(
                { q: null, status: "enabled" },
                { replace: true },
            )
        })
        expect(mockReplace).toHaveBeenCalledWith(
            "/system/access-audit?view=roles&status=enabled",
        )
    })
})

describe("useAccessListControls", () => {
    const patchUrl = vi.fn()
    const patchFilterUrl = vi.fn()
    const resetPaginationToFirstPage = vi.fn()

    it("initializes search from qParam and syncs when it changes", () => {
        const { result, rerender } = renderHook(
            ({ qParam }: { qParam: string }) =>
                useAccessListControls({
                    qParam,
                    patchUrl,
                    patchFilterUrl,
                    resetPaginationToFirstPage,
                }),
            {
                wrapper: wrapperWithClient(),
                initialProps: { qParam: "abc" },
            },
        )
        expect(result.current.searchInput).toBe("abc")

        rerender({ qParam: "next" })
        expect(result.current.searchInput).toBe("next")
    })

    it("debounces search input and patches the URL with q plus page reset", async () => {
        vi.useFakeTimers()
        try {
            const { result } = renderHookWithProviders(() =>
                useAccessListControls({
                    qParam: "",
                    patchUrl,
                    patchFilterUrl,
                    resetPaginationToFirstPage,
                }),
            )
            act(() => {
                result.current.setSearchInput("新关键词")
            })
            act(() => {
                vi.advanceTimersByTime(300)
            })
            expect(patchUrl).toHaveBeenCalledWith(
                { q: "新关键词", page: null },
                { replace: true },
            )
            expect(resetPaginationToFirstPage).toHaveBeenCalled()
        } finally {
            vi.useRealTimers()
        }
    })

    it("debounces advanced filters and calls patchFilterUrl once", async () => {
        vi.useFakeTimers()
        try {
            const { result } = renderHookWithProviders(() =>
                useAccessListControls({
                    qParam: "",
                    patchUrl,
                    patchFilterUrl,
                    resetPaginationToFirstPage,
                }),
            )
            act(() => {
                result.current.setDebouncedFilters({ actorId: "张三" })
            })
            act(() => {
                vi.advanceTimersByTime(300)
            })
            expect(patchFilterUrl).toHaveBeenCalledWith({
                actorId: "张三",
                traceId: null,
                objectType: null,
                objectId: null,
            })
        } finally {
            vi.useRealTimers()
        }
    })
})

describe("useAccessDetailPanels", () => {
    const patchUrl = vi.fn()

    beforeEach(() => {
        patchUrl.mockClear()
    })

    it("initializes explain state only for subject views and keeps it until the URL patch clears it", () => {
        const { result, rerender } = renderHook(
            ({ params }: { params: Parameters<typeof useAccessDetailPanels>[0] }) =>
                useAccessDetailPanels(params),
            {
                wrapper: wrapperWithClient(),
                initialProps: {
                    params: {
                        view: "users",
                        subjectTypeParam: "USER",
                        subjectIdParam: "u1",
                        patchUrl,
                    },
                },
            },
        )
        expect(result.current.explainSubject).toEqual({
            type: "USER",
            id: "u1",
        })

        rerender({
            params: {
                view: "audit",
                subjectTypeParam: "USER",
                subjectIdParam: "u1",
                patchUrl,
            },
        })
        // 仅挂载时按 URL 初始化；view 切换的清理由 switchView 显式完成
        expect(result.current.explainSubject).toEqual({
            type: "USER",
            id: "u1",
        })
    })

    it("opens and closes the explain panel patching the URL", () => {
        const { result } = renderHookWithProviders(() =>
            useAccessDetailPanels({
                view: "roles",
                patchUrl,
            }),
        )
        act(() => {
            result.current.openExplain("ROLE", "role_1")
        })
        expect(result.current.explainSubject).toEqual({
            type: "ROLE",
            id: "role_1",
        })
        expect(patchUrl).toHaveBeenCalledWith(
            { subjectType: "ROLE", subjectId: "role_1", eventId: null },
            { replace: true },
        )

        act(() => {
            result.current.closeExplain()
        })
        expect(result.current.explainSubject).toBeNull()
        expect(patchUrl).toHaveBeenCalledWith(
            { subjectId: null, subjectType: null },
            { replace: true },
        )
    })

    it("opens and closes the event panel patching the URL", async () => {
        vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
            cb(0)
            return 0
        })
        const { result } = renderHookWithProviders(() =>
            useAccessDetailPanels({
                view: "audit",
                eventIdParam: "ae_1",
                patchUrl,
            }),
        )
        await waitFor(() => expect(result.current.eventOpenId).toBe("ae_1"))

        act(() => {
            result.current.openEvent("ae_2")
        })
        expect(result.current.eventOpenId).toBe("ae_2")
        expect(patchUrl).toHaveBeenCalledWith(
            { eventId: "ae_2" },
            { replace: true },
        )

        act(() => {
            result.current.closeEvent()
        })
        expect(result.current.eventOpenId).toBeNull()
        expect(patchUrl).toHaveBeenCalledWith(
            { eventId: null },
            { replace: true },
        )
        vi.unstubAllGlobals()
    })
})
