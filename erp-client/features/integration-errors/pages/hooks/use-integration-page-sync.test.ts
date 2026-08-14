import { renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { parseIntegrationSearchParams } from "../../lib/url-state"
import { makeItem, makeQueueView } from "./test-fixtures"
import { useIntegrationPageSync } from "./use-integration-page-sync"

const mocks = vi.hoisted(() => ({
    replace: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: vi.fn(),
        replace: mocks.replace,
        back: vi.fn(),
    }),
    useSearchParams: () => mocks.searchParams,
    usePathname: () => "/governance/integration-errors",
    useParams: () => ({}),
}))

beforeEach(() => {
    mocks.replace.mockClear()
    mocks.searchParams = new URLSearchParams()
})

const urlState = () =>
    parseIntegrationSearchParams(new URLSearchParams("view=mine"))

function renderSync(overrides: Partial<{
    focusMode: boolean
    view: ReturnType<typeof makeQueueView> | undefined
    queuePending: boolean
    urlState: ReturnType<typeof urlState>
    item: ReturnType<typeof makeItem> | undefined
    itemCount: number
    autoNext: boolean
}> = {}) {
    return renderHook(() =>
        useIntegrationPageSync({
            focusMode: overrides.focusMode ?? false,
            view: overrides.view,
            queuePending: overrides.queuePending ?? false,
            urlState: overrides.urlState ?? urlState(),
            item: overrides.item,
            itemCount: overrides.itemCount ?? 1,
            autoNext: overrides.autoNext ?? false,
        }),
    )
}

function paramsOf(url: unknown): URLSearchParams {
    expect(typeof url).toBe("string")
    return new URL(url as string, "http://test").searchParams
}

describe("useIntegrationPageSync", () => {
    it("redirects to the error-task detail after resolveWorkItemId succeeds", () => {
        const state = parseIntegrationSearchParams(
            new URLSearchParams("view=mine&resolveWorkItemId=rw1"),
        )
        const view = makeQueueView({
            resolvedEntry: {
                itemType: "ERROR_TASK",
                id: "t1",
                workItemId: "rw1",
            },
        })
        renderSync({ view, urlState: state, autoNext: true })
        expect(mocks.replace).toHaveBeenCalledWith(
            `/governance/integration-errors/errors/t1?queueContextId=${encodeURIComponent(state.queueContextId)}&view=mine&autoNext=1`,
        )
    })

    it("redirects to the difference detail after resolveWorkItemId succeeds", () => {
        const state = parseIntegrationSearchParams(
            new URLSearchParams("view=reconciliation&resolveWorkItemId=rw2"),
        )
        const view = makeQueueView({
            resolvedEntry: {
                itemType: "RECONCILIATION_DIFFERENCE",
                id: "d1",
                workItemId: "rw2",
            },
        })
        renderSync({ view, urlState: state })
        const url = mocks.replace.mock.calls[0][0] as string
        expect(url.startsWith("/governance/integration-errors/differences/d1?")).toBe(true)
        expect(paramsOf(url).get("view")).toBe("reconciliation")
        expect(paramsOf(url).get("autoNext")).toBe("0")
    })

    it("does not redirect without resolveWorkItemId", () => {
        const view = makeQueueView({
            resolvedEntry: {
                itemType: "ERROR_TASK",
                id: "t1",
                workItemId: "rw1",
            },
        })
        renderSync({ view })
        const urls = mocks.replace.mock.calls.map((call) => String(call[0]))
        expect(urls.some((url) => url.includes("/errors/"))).toBe(false)
        expect(urls.some((url) => url.includes("/differences/"))).toBe(false)
    })

    it("writes the current task id as URL default when params are missing", () => {
        const item = makeItem()
        renderSync({ view: makeQueueView(), item })
        expect(mocks.replace).toHaveBeenCalledTimes(1)
        const url = mocks.replace.mock.calls[0][0] as string
        expect(url.startsWith("/governance/integration-errors?")).toBe(true)
        expect(paramsOf(url).get("taskId")).toBe("task-1")
    })

    it("writes the current difference id as URL default for differences", () => {
        const item = makeItem({
            identity: {
                itemType: "RECONCILIATION_DIFFERENCE",
                id: "diff-1",
                number: "RD-1",
                subjectHash: "h3",
            },
        })
        renderSync({ view: makeQueueView(), item })
        const url = mocks.replace.mock.calls[0][0] as string
        expect(paramsOf(url).get("differenceId")).toBe("diff-1")
    })

    it("skips URL defaults when view, context and selection params exist", () => {
        mocks.searchParams = new URLSearchParams(
            "view=mine&queueContextId=queue:W29:mine:all&taskId=t1",
        )
        const state = parseIntegrationSearchParams(mocks.searchParams)
        renderSync({
            view: makeQueueView(),
            urlState: state,
            item: makeItem(),
        })
        expect(mocks.replace).not.toHaveBeenCalled()
    })

    it("skips URL defaults in focus mode", () => {
        renderSync({
            focusMode: true,
            view: makeQueueView(),
            item: makeItem(),
        })
        expect(mocks.replace).not.toHaveBeenCalled()
    })

    it("skips URL defaults while the queue is pending", () => {
        renderSync({
            queuePending: true,
            view: makeQueueView(),
            item: makeItem(),
        })
        expect(mocks.replace).not.toHaveBeenCalled()
    })

    it("skips URL defaults while a resolveWorkItemId is being applied", () => {
        const state = parseIntegrationSearchParams(
            new URLSearchParams("view=mine&resolveWorkItemId=rw1"),
        )
        renderSync({
            view: makeQueueView(),
            urlState: state,
            item: makeItem(),
        })
        expect(mocks.replace).not.toHaveBeenCalled()
    })
})
