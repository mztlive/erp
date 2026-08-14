import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { makeItem } from "./test-fixtures"
import { useIntegrationItemNavigation } from "./use-integration-item-navigation"

const replaceUrl = vi.fn()
const onBeforeNavigate = vi.fn()

beforeEach(() => {
    replaceUrl.mockClear()
    onBeforeNavigate.mockClear()
})

const first = makeItem()
const second = makeItem({
    identity: {
        itemType: "ERROR_TASK",
        id: "task-2",
        number: "ET-2",
        subjectHash: "h2",
    },
})
const difference = makeItem({
    identity: {
        itemType: "RECONCILIATION_DIFFERENCE",
        id: "diff-1",
        number: "RD-1",
        subjectHash: "h3",
    },
})

function renderNavigation(overrides: Partial<{
    items: typeof first[]
    queueItems: typeof first[]
    item: typeof first | undefined
    focusMode: boolean
}> = {}) {
    const items = overrides.items ?? [first, second]
    const item = overrides.item === undefined ? second : overrides.item
    return renderHook(() =>
        useIntegrationItemNavigation({
            items,
            queueItems: overrides.queueItems ?? items,
            item,
            focusMode: overrides.focusMode ?? false,
            replaceUrl,
            onBeforeNavigate,
        }),
    )
}

describe("useIntegrationItemNavigation", () => {
    it("derives position indexes from the display list", () => {
        const { result } = renderNavigation()
        expect(result.current.currentIndex).toBe(1)
        expect(result.current.queueIndex).toBe(1)
        expect(result.current.positionIndex).toBe(2)
        expect(result.current.positionTotal).toBe(2)
    })

    it("derives focus-mode position from the queue", () => {
        const { result } = renderNavigation({
            items: [second],
            queueItems: [first, second, difference],
            item: second,
            focusMode: true,
        })
        expect(result.current.positionIndex).toBe(2)
        expect(result.current.positionTotal).toBe(3)
    })

    it("navigates to a task by taskId and clears the transient state", () => {
        const { result } = renderNavigation()
        act(() => {
            result.current.goToItem(second)
        })
        expect(onBeforeNavigate).toHaveBeenCalledTimes(1)
        expect(replaceUrl).toHaveBeenCalledWith({
            taskId: "task-2",
            differenceId: null,
        })
    })

    it("navigates to a difference by differenceId", () => {
        const { result } = renderNavigation()
        act(() => {
            result.current.goToItem(difference)
        })
        expect(replaceUrl).toHaveBeenCalledWith({
            differenceId: "diff-1",
            taskId: null,
        })
    })

    it("clears the selection when navigating to null", () => {
        const { result } = renderNavigation()
        act(() => {
            result.current.goToItem(null)
        })
        expect(onBeforeNavigate).toHaveBeenCalledTimes(1)
        expect(replaceUrl).toHaveBeenCalledWith({
            taskId: null,
            differenceId: null,
        })
    })

    it("returns neighbors within bounds and null outside", () => {
        const { result } = renderNavigation({ item: first })
        expect(result.current.neighbor(-1)).toBeNull()
        expect(result.current.neighbor(1)).toBe(second)
        expect(result.current.neighbor(2)).toBeNull()
    })
})
