import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, renderHook } from "@testing-library/react"

const mocks = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
    searchParams: { current: new URLSearchParams() },
    pathname: { current: "/test" },
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: mocks.push,
        replace: mocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => mocks.searchParams.current,
    usePathname: () => mocks.pathname.current,
    useParams: () => ({}),
}))

import {
    parseSection,
    SECTIONS,
    usePublicationCenterUrlState,
} from "./publication-center-navigation"

function renderUrlState(dirty = false, clearSessionEdit = vi.fn()) {
    return renderHook(() =>
        usePublicationCenterUrlState({ dirty, clearSessionEdit }),
    )
}

beforeEach(() => {
    mocks.push.mockClear()
    mocks.replace.mockClear()
    mocks.searchParams.current = new URLSearchParams()
    mocks.pathname.current = "/test"
})

describe("parseSection", () => {
    it("maps known section ids and falls back to overview", () => {
        expect(parseSection("media")).toBe("media")
        expect(parseSection("delivery")).toBe("delivery")
        expect(parseSection("unknown")).toBe("overview")
        expect(parseSection(null)).toBe("overview")
    })

    it("all sections have distinct ids", () => {
        const ids = SECTIONS.map((s) => s.id)
        expect(new Set(ids).size).toBe(ids.length)
    })
})

describe("usePublicationCenterUrlState", () => {
    it("defaults to overview with no revision param", () => {
        const { result } = renderUrlState()
        expect(result.current.section).toBe("overview")
        expect(result.current.revisionParam).toBeUndefined()
    })

    it("parses section and revision from the URL", () => {
        mocks.searchParams.current = new URLSearchParams(
            "section=delivery&revision=rev_9",
        )
        const { result } = renderUrlState()
        expect(result.current.section).toBe("delivery")
        expect(result.current.revisionParam).toBe("rev_9")
    })

    it("treats invalid section as overview but keeps other params", () => {
        mocks.searchParams.current = new URLSearchParams(
            "section=nope&revision=rev_1",
        )
        const { result } = renderUrlState()
        expect(result.current.section).toBe("overview")
        expect(result.current.revisionParam).toBe("rev_1")
    })

    it("setSection to overview removes the section param and preserves others", () => {
        mocks.searchParams.current = new URLSearchParams("section=media&q=1")
        const { result } = renderUrlState()
        act(() => result.current.setSection("overview"))
        expect(mocks.replace).toHaveBeenCalledWith("/test?q=1")
    })

    it("setSection sets the param on a bare path", () => {
        const { result } = renderUrlState()
        act(() => result.current.setSection("content"))
        expect(mocks.replace).toHaveBeenCalledWith("/test?section=content")
    })

    it("setSection replaces an existing section value", () => {
        mocks.searchParams.current = new URLSearchParams(
            "section=media&revision=rev_2",
        )
        const { result } = renderUrlState()
        act(() => result.current.setSection("audit"))
        expect(mocks.replace).toHaveBeenCalledWith(
            "/test?section=audit&revision=rev_2",
        )
    })

    it("clearRevision removes only the revision param", () => {
        mocks.searchParams.current = new URLSearchParams(
            "section=delivery&revision=rev_1",
        )
        const { result } = renderUrlState()
        act(() => result.current.clearRevision())
        expect(mocks.replace).toHaveBeenCalledWith("/test?section=delivery")
    })

    it("clearRevision falls back to the bare path when nothing else remains", () => {
        mocks.searchParams.current = new URLSearchParams("revision=rev_1")
        const { result } = renderUrlState()
        act(() => result.current.clearRevision())
        expect(mocks.replace).toHaveBeenCalledWith("/test")
    })

    it("selectRevision without dirty jumps to the delivery section with the revision", () => {
        mocks.searchParams.current = new URLSearchParams("section=media")
        const { result } = renderUrlState()
        act(() => result.current.selectRevision("rev_3"))
        expect(mocks.replace).toHaveBeenCalledWith(
            "/test?section=delivery&revision=rev_3",
        )
    })

    it("selectRevision with dirty asks before discarding and aborts on cancel", () => {
        const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false)
        const clearSessionEdit = vi.fn()
        const { result } = renderUrlState(true, clearSessionEdit)
        act(() => result.current.selectRevision("rev_3"))
        expect(confirmSpy).toHaveBeenCalledWith(
            "切换历史修订将放弃本次未提交输入。输入仅存在于当前页签，不会保存草稿。",
        )
        expect(clearSessionEdit).not.toHaveBeenCalled()
        expect(mocks.replace).not.toHaveBeenCalled()
        confirmSpy.mockRestore()
    })

    it("selectRevision with dirty confirmation clears the session and navigates", () => {
        vi.spyOn(window, "confirm").mockReturnValue(true)
        const clearSessionEdit = vi.fn()
        const { result } = renderUrlState(true, clearSessionEdit)
        act(() => result.current.selectRevision("rev_3"))
        expect(clearSessionEdit).toHaveBeenCalledTimes(1)
        expect(mocks.replace).toHaveBeenCalledWith(
            "/test?section=delivery&revision=rev_3",
        )
        vi.restoreAllMocks()
    })
})
