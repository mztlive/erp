import { act, renderHook, waitFor } from "@testing-library/react"
import { QueryClientProvider } from "@tanstack/react-query"
import * as React from "react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { useAccessDetailPanels } from "./use-access-detail-panels"
import { useAccessListFilters } from "./use-access-list-filters"
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

describe("useAccessListFilters", () => {
    const patchFilterUrl = vi.fn()
    const searchInputRef = {
        current: null,
    } as React.RefObject<HTMLInputElement | null>

    const renderFilters = (view: "roles" | "audit" | "scopes" = "audit") =>
        renderHookWithProviders(() =>
            useAccessListFilters({ view, patchFilterUrl, searchInputRef }),
        )

    beforeEach(() => {
        patchFilterUrl.mockClear()
    })

    it("initializes the draft from the URL and opens the panel for structured deep links", () => {
        currentSearchParams = "view=audit&q=abc&result=SUCCESS&from=2026-01-01"
        const { result } = renderFilters()

        expect(result.current.applied.q).toBe("abc")
        expect(result.current.applied.result).toBe("SUCCESS")
        expect(result.current.draft.q).toBe("abc")
        expect(result.current.draft.result).toBe("SUCCESS")
        expect(result.current.draft.from).toBe("2026-01-01")
        expect(result.current.panelOpen).toBe(true)
    })

    it("degrades invalid enum values and keeps the panel closed without structured filters", () => {
        currentSearchParams = "view=roles&status=weird"
        const { result } = renderFilters("roles")

        expect(result.current.applied.status).toBeUndefined()
        expect(result.current.draft.status).toBe("all")
        expect(result.current.panelOpen).toBe(false)
        expect(result.current.hasStructuredFilters).toBe(false)
    })

    it("keeps the panel closed for the scopes view when only the subject lock exists", () => {
        currentSearchParams = "view=scopes&subjectType=USER&subjectId=u1"
        const { result } = renderFilters("scopes")

        expect(result.current.panelOpen).toBe(false)
    })

    it("does not patch the URL while editing drafts", () => {
        currentSearchParams = "view=audit"
        const { result } = renderFilters()

        act(() => {
            result.current.setSearchDraft("abc")
            result.current.updateDraft("actorId", "张三")
        })
        expect(patchFilterUrl).not.toHaveBeenCalled()
    })

    it("applies all drafts in one patch, resets to page 1 and closes the panel", () => {
        currentSearchParams = "view=audit"
        const { result } = renderFilters()

        act(() => result.current.updateDraft("q", " 关键词 "))
        act(() => result.current.updateDraft("status", "enabled"))
        act(() => result.current.updateDraft("result", "DENIED"))
        act(() => result.current.updateDraft("from", "2026-01-01"))
        act(() => result.current.updateDraft("to", "2026-01-02"))
        act(() => result.current.updateDraft("actorId", "张三"))
        act(() => result.current.applyFilters())
        expect(patchFilterUrl).toHaveBeenCalledWith({
            q: "关键词",
            org: null,
            status: "enabled",
            risk: null,
            from: "2026-01-01",
            to: "2026-01-02",
            action: null,
            result: "DENIED",
            actorId: "张三",
            traceId: null,
            objectType: null,
            objectId: null,
            page: null,
        })
        expect(result.current.panelOpen).toBe(false)
    })

    it("validates the audit date range on submit and keeps the panel open on error", () => {
        currentSearchParams = "view=audit"
        const { result } = renderFilters()

        act(() => result.current.updateDraft("from", "2026-01-02"))
        act(() => result.current.updateDraft("to", "2026-01-01"))
        act(() => result.current.applyFilters())
        expect(patchFilterUrl).not.toHaveBeenCalled()
        expect(result.current.filterError).toBe("截止日期不能早于起始日期")
        expect(result.current.panelOpen).toBe(true)
    })

    it("clears everything through clearAllFilters but keeps the view", () => {
        currentSearchParams = "view=audit&q=x&from=2026-01-01"
        const { result } = renderFilters()

        act(() => {
            result.current.clearAllFilters()
        })
        expect(patchFilterUrl).toHaveBeenCalledWith({
            q: null,
            org: null,
            status: null,
            risk: null,
            from: null,
            to: null,
            action: null,
            result: null,
            actorId: null,
            traceId: null,
            objectType: null,
            objectId: null,
            page: null,
        })
        expect(result.current.draft.q).toBe("")
        expect(result.current.panelOpen).toBe(false)
    })

    it("clears the scopes subject lock together with all filters", () => {
        currentSearchParams = "view=scopes&subjectType=USER&subjectId=u1"
        const { result } = renderFilters("scopes")

        act(() => {
            result.current.clearAllFilters()
        })
        expect(patchFilterUrl).toHaveBeenCalledWith(
            expect.objectContaining({ subjectType: null, subjectId: null }),
        )
    })

    it("keeps the subject lock when clearing filters outside the scopes view", () => {
        currentSearchParams = "view=roles&subjectType=ROLE&subjectId=role_1"
        const { result } = renderFilters("roles")

        act(() => {
            result.current.clearAllFilters()
        })
        expect(patchFilterUrl).toHaveBeenCalledWith(
            expect.not.objectContaining({ subjectType: expect.anything() }),
        )
    })

    it("resetMoreFilters clears only structured conditions and keeps the panel open", () => {
        currentSearchParams = "view=audit&q=abc&actorId=a1"
        const { result } = renderFilters()

        act(() => {
            result.current.setPanelOpen(true)
        })
        act(() => result.current.resetMoreFilters())
        expect(patchFilterUrl).toHaveBeenCalledWith({
            org: null,
            status: null,
            risk: null,
            from: null,
            to: null,
            action: null,
            result: null,
            actorId: null,
            traceId: null,
            objectType: null,
            objectId: null,
            page: null,
        })
        expect(result.current.panelOpen).toBe(true)
        expect(result.current.draft.q).toBe("abc")
    })

    it("removes a single applied condition through removeFilter", () => {
        currentSearchParams = "view=audit&from=2026-01-01&to=2026-01-02"
        const { result } = renderFilters()

        act(() => {
            result.current.removeFilter("time")
        })
        expect(patchFilterUrl).toHaveBeenCalledWith({
            from: null,
            to: null,
            page: null,
        })
    })

    it("backfills drafts from the URL without reopening the panel", () => {
        currentSearchParams = "view=audit&result=SUCCESS"
        const { result, rerender } = renderFilters()
        expect(result.current.panelOpen).toBe(true)

        act(() => {
            result.current.setPanelOpen(false)
        })
        currentSearchParams = "view=audit&result=DENIED&actorId=a1"
        rerender()

        expect(result.current.draft.result).toBe("DENIED")
        expect(result.current.draft.actorId).toBe("a1")
        expect(result.current.panelOpen).toBe(false)
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
