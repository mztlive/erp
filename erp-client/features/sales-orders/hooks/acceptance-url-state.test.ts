import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { useAcceptanceWorkspaceUrlState } from "./acceptance-url-state"

vi.mock("next/navigation", () => ({
    useRouter: vi.fn(() => ({
        push: vi.fn(),
        replace: vi.fn(),
        back: vi.fn(),
    })),
    useSearchParams: vi.fn(() => new URLSearchParams()),
    usePathname: vi.fn(() => "/test"),
    useParams: vi.fn(() => ({})),
}))

import { usePathname, useRouter, useSearchParams } from "next/navigation"

type MockedSearchParams = ReturnType<typeof useSearchParams>
type MockedRouter = ReturnType<typeof useRouter>

function setup(query: string, pathname = "/sales/orders/so_1") {
    const replace = vi.fn()
    vi.mocked(useRouter).mockReturnValue({
        push: vi.fn(),
        replace,
        back: vi.fn(),
    } as unknown as MockedRouter)
    vi.mocked(usePathname).mockReturnValue(pathname)
    vi.mocked(useSearchParams).mockReturnValue(
        new URLSearchParams(query) as unknown as MockedSearchParams,
    )
    return { replace }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useAcceptanceWorkspaceUrlState", () => {
    it("defaults remainingOnly to true when the param is absent", () => {
        setup("")
        const { result } = renderHook(() => useAcceptanceWorkspaceUrlState())

        expect(result.current.remainingOnly).toBe(true)
        expect(result.current.workItemId).toBeNull()
    })

    it("parses workItemId and remainingOnly=false from the URL", () => {
        setup("workItemId=wi_1&remainingOnly=false")
        const { result } = renderHook(() => useAcceptanceWorkspaceUrlState())

        expect(result.current.workItemId).toBe("wi_1")
        expect(result.current.remainingOnly).toBe(false)
    })

    it("treats any remainingOnly value other than the literal false as true", () => {
        setup("remainingOnly=1")
        const first = renderHook(() => useAcceptanceWorkspaceUrlState())
        expect(first.result.current.remainingOnly).toBe(true)
        first.unmount()

        setup("remainingOnly=true")
        const second = renderHook(() => useAcceptanceWorkspaceUrlState())
        expect(second.result.current.remainingOnly).toBe(true)
    })

    it("switches to all history and persists section + remainingOnly=0", () => {
        const { replace } = setup("workItemId=wi_1&remainingOnly=false")
        const { result } = renderHook(() => useAcceptanceWorkspaceUrlState())

        act(() => result.current.setRemainingOnly(false))

        expect(result.current.remainingOnly).toBe(false)
        expect(replace).toHaveBeenCalledTimes(1)
        expect(replace).toHaveBeenCalledWith(
            "/sales/orders/so_1?workItemId=wi_1&remainingOnly=0&section=acceptance",
            { scroll: false },
        )
    })

    it("switches to remaining-only filter with remainingOnly=1", () => {
        const { replace } = setup("")
        const { result } = renderHook(() => useAcceptanceWorkspaceUrlState())

        act(() => result.current.setRemainingOnly(true))

        expect(result.current.remainingOnly).toBe(true)
        expect(replace).toHaveBeenCalledWith(
            "/sales/orders/so_1?section=acceptance&remainingOnly=1",
            { scroll: false },
        )
    })
})
