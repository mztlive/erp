import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { useIntegrationPageUrl } from "./use-integration-page-url"

const mocks = vi.hoisted(() => ({
    replace: vi.fn(),
    searchParams: new URLSearchParams(),
    pathname: "/governance/integration-errors",
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: vi.fn(),
        replace: mocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => mocks.searchParams,
    usePathname: () => mocks.pathname,
    useParams: () => ({}),
}))

beforeEach(() => {
    mocks.replace.mockClear()
    mocks.searchParams = new URLSearchParams()
    mocks.pathname = "/governance/integration-errors"
})

function paramsOf(url: unknown): URLSearchParams {
    expect(typeof url).toBe("string")
    return new URL(url as string, "http://test").searchParams
}

describe("useIntegrationPageUrl", () => {
    it("derives defaults from an empty URL", () => {
        const { result } = renderHook(() => useIntegrationPageUrl({}))
        expect(result.current.focusMode).toBe(false)
        expect(result.current.urlState.view).toBe("mine")
        expect(result.current.urlState.mode).toBe("all")
        expect(result.current.urlState.environment).toBe("production")
        expect(result.current.urlState.owner).toBe("me")
        expect(result.current.urlState.autoNext).toBe(false)
        expect(result.current.urlState.queueContextId).toBe("queue:W29:mine:all")
        expect(result.current.currentTaskId).toBeUndefined()
        expect(result.current.currentDifferenceId).toBeUndefined()
        expect(result.current.query).toMatchObject({
            view: "mine",
            queueContextId: "queue:W29:mine:all",
        })
        expect(result.current.hasQueueFilters).toBe(false)
    })

    it("parses selection and preference params into the query", () => {
        mocks.searchParams = new URLSearchParams(
            "view=security&taskId=t1&autoNext=1",
        )
        const { result } = renderHook(() => useIntegrationPageUrl({}))
        expect(result.current.currentTaskId).toBe("t1")
        expect(result.current.urlState.autoNext).toBe(true)
        expect(result.current.autoNext).toBe(true)
        expect(result.current.query).toMatchObject({
            view: "security",
            currentTaskId: "t1",
            autoNext: true,
        })
        expect(result.current.hasQueueFilters).toBe(false)
    })

    it("derives hasQueueFilters from non-default filters", () => {
        mocks.searchParams = new URLSearchParams("errorClass=rate-limited")
        const { result } = renderHook(() => useIntegrationPageUrl({}))
        expect(result.current.hasQueueFilters).toBe(true)
    })

    it("forces the focus mode from forcedTaskId", () => {
        const { result } = renderHook(() =>
            useIntegrationPageUrl({ forcedTaskId: "f1" }),
        )
        expect(result.current.focusMode).toBe(true)
        expect(result.current.currentTaskId).toBe("f1")
        expect(result.current.query.currentTaskId).toBe("f1")
    })

    it("forces the focus mode from forcedDifferenceId", () => {
        const { result } = renderHook(() =>
            useIntegrationPageUrl({ forcedDifferenceId: "fd1" }),
        )
        expect(result.current.focusMode).toBe(true)
        expect(result.current.currentDifferenceId).toBe("fd1")
    })

    it("writes view patches into the list URL and clears the selection", () => {
        const { result } = renderHook(() => useIntegrationPageUrl({}))
        act(() => {
            result.current.replaceUrl({
                view: "security",
                taskId: null,
                differenceId: null,
            })
        })
        expect(mocks.replace).toHaveBeenCalledTimes(1)
        const url = mocks.replace.mock.calls[0][0] as string
        expect(url.startsWith("/governance/integration-errors?")).toBe(true)
        const params = paramsOf(url)
        expect(params.get("view")).toBe("security")
        expect(params.has("taskId")).toBe(false)
        expect(params.has("differenceId")).toBe(false)
    })

    it("keeps unspecified params when patching the list URL", () => {
        mocks.searchParams = new URLSearchParams("errorClass=rate-limited")
        const { result } = renderHook(() => useIntegrationPageUrl({}))
        act(() => {
            result.current.replaceUrl({ owner: "team", taskId: null })
        })
        const params = paramsOf(mocks.replace.mock.calls[0][0])
        expect(params.get("owner")).toBe("team")
        expect(params.get("errorClass")).toBe("rate-limited")
        expect(params.has("taskId")).toBe(false)
    })

    it("maps the autoNext patch to the URL param", () => {
        const { result } = renderHook(() => useIntegrationPageUrl({}))
        act(() => {
            result.current.replaceUrl({ autoNext: "1" })
        })
        expect(paramsOf(mocks.replace.mock.calls[0][0]).get("autoNext")).toBe("1")
        act(() => {
            result.current.replaceUrl({ autoNext: "0" })
        })
        expect(paramsOf(mocks.replace.mock.calls[1][0]).get("autoNext")).toBe("0")
    })

    it("drops resolveWorkItemId after it is applied", () => {
        mocks.searchParams = new URLSearchParams(
            "view=mine&resolveWorkItemId=rw1",
        )
        const { result } = renderHook(() => useIntegrationPageUrl({}))
        act(() => {
            result.current.replaceUrl({ resolveWorkItemId: null })
        })
        const params = paramsOf(mocks.replace.mock.calls[0][0])
        expect(params.has("resolveWorkItemId")).toBe(false)
    })

    it("ignores selection patches in focus mode but keeps query prefs", () => {
        mocks.pathname = "/governance/integration-errors/errors/f1"
        mocks.searchParams = new URLSearchParams("view=security")
        const { result } = renderHook(() =>
            useIntegrationPageUrl({ forcedTaskId: "f1" }),
        )
        act(() => {
            result.current.replaceUrl({
                view: "mine",
                taskId: "other",
                differenceId: "other",
            })
        })
        expect(mocks.replace).toHaveBeenCalledTimes(1)
        const [url, options] = mocks.replace.mock.calls[0]
        expect(url).toBe(
            "/governance/integration-errors/errors/f1?view=mine",
        )
        expect(options).toEqual({ scroll: false })
    })
})
