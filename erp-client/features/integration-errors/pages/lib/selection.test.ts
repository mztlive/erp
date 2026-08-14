import { describe, expect, it } from "vitest"

import { makeItem } from "../hooks/test-fixtures"
import {
    derivePosition,
    deriveResponsibilityStatus,
    resolveDetailTarget,
    resolveDisplayItem,
    selectQueueSelection,
} from "./selection"

describe("selectQueueSelection", () => {
    it("prefers an ERROR_TASK with the current task id over same-id items", () => {
        const task = makeItem({ identity: { itemType: "ERROR_TASK", id: "t1", number: "ET-1", subjectHash: "h1" } })
        const diff = makeItem({
            identity: { itemType: "RECONCILIATION_DIFFERENCE", id: "t1", number: "RD-1", subjectHash: "h2" },
        })
        const selected = selectQueueSelection([diff, task], "t1", undefined)
        expect(selected).toBe(task)
    })

    it("falls back to any item with the same id when no task matches", () => {
        const diff = makeItem({
            identity: { itemType: "RECONCILIATION_DIFFERENCE", id: "d1", number: "RD-1", subjectHash: "h2" },
        })
        const selected = selectQueueSelection([diff], "d1", undefined)
        expect(selected).toBe(diff)
    })

    it("selects a RECONCILIATION_DIFFERENCE by difference id", () => {
        const task = makeItem()
        const diff = makeItem({
            identity: { itemType: "RECONCILIATION_DIFFERENCE", id: "d1", number: "RD-1", subjectHash: "h2" },
        })
        const selected = selectQueueSelection([task, diff], undefined, "d1")
        expect(selected).toBe(diff)
    })

    it("returns the first item when no id filter is given", () => {
        const first = makeItem()
        const second = makeItem({
            identity: { itemType: "ERROR_TASK", id: "task-2", number: "ET-2", subjectHash: "h2" },
        })
        expect(selectQueueSelection([first, second], undefined, undefined)).toBe(first)
    })

    it("returns undefined for an empty queue", () => {
        expect(selectQueueSelection([], "t1", undefined)).toBeUndefined()
    })
})

describe("resolveDetailTarget", () => {
    it("prioritizes the forced task id", () => {
        const queueSelection = makeItem()
        expect(resolveDetailTarget("f1", undefined, queueSelection)).toEqual({
            itemType: "ERROR_TASK",
            id: "f1",
        })
    })

    it("uses the forced difference id", () => {
        const queueSelection = makeItem()
        expect(resolveDetailTarget(undefined, "fd1", queueSelection)).toEqual({
            itemType: "RECONCILIATION_DIFFERENCE",
            id: "fd1",
        })
    })

    it("derives the target from the queue selection", () => {
        const queueSelection = makeItem({
            identity: { itemType: "ERROR_TASK", id: "task-1", number: "ET-1", subjectHash: "h1" },
        })
        expect(resolveDetailTarget(undefined, undefined, queueSelection)).toEqual({
            itemType: "ERROR_TASK",
            id: "task-1",
        })
    })

    it("returns null without a forced id or selection", () => {
        expect(resolveDetailTarget(undefined, undefined, undefined)).toBeNull()
    })
})

describe("resolveDisplayItem", () => {
    it("returns detail data when it matches the target", () => {
        const queueSelection = makeItem()
        const detailData = makeItem({
            identity: { itemType: "ERROR_TASK", id: "task-1", number: "ET-1", subjectHash: "h1" },
        })
        expect(
            resolveDisplayItem({ itemType: "ERROR_TASK", id: "task-1" }, detailData, queueSelection),
        ).toBe(detailData)
    })

    it("falls back to the queue selection on identity mismatch", () => {
        const queueSelection = makeItem()
        const detailData = makeItem({
            identity: { itemType: "ERROR_TASK", id: "other", number: "ET-9", subjectHash: "h9" },
        })
        expect(
            resolveDisplayItem({ itemType: "ERROR_TASK", id: "task-1" }, detailData, queueSelection),
        ).toBe(queueSelection)
    })

    it("falls back to the queue selection when detail data is missing", () => {
        const queueSelection = makeItem()
        expect(
            resolveDisplayItem({ itemType: "ERROR_TASK", id: "task-1" }, null, queueSelection),
        ).toBe(queueSelection)
    })

    it("returns the queue selection when there is no detail target", () => {
        const queueSelection = makeItem()
        expect(resolveDisplayItem(null, undefined, queueSelection)).toBe(queueSelection)
    })
})

describe("derivePosition", () => {
    const second = makeItem({
        identity: { itemType: "ERROR_TASK", id: "task-2", number: "ET-2", subjectHash: "h2" },
    })
    const third = makeItem({
        identity: { itemType: "ERROR_TASK", id: "task-3", number: "ET-3", subjectHash: "h3" },
    })

    it("derives list-mode indexes from the display list", () => {
        const items = [makeItem(), second, third]
        expect(derivePosition(second, items, items, false)).toEqual({
            currentIndex: 1,
            queueIndex: 1,
            positionIndex: 2,
            positionTotal: 3,
        })
    })

    it("clamps currentIndex to 0 when the item is not in the list", () => {
        const items = [makeItem(), second]
        expect(derivePosition(third, items, items, false)).toEqual({
            currentIndex: 0,
            queueIndex: -1,
            positionIndex: 1,
            positionTotal: 2,
        })
    })

    it("uses queue position in focus mode when the item is in the queue", () => {
        const items = [third]
        const queueItems = [makeItem(), second, third]
        expect(derivePosition(third, items, queueItems, true)).toEqual({
            currentIndex: 0,
            queueIndex: 2,
            positionIndex: 3,
            positionTotal: 3,
        })
    })

    it("shows 1/1 in focus mode when the item is not in the queue", () => {
        const items = [third]
        const queueItems = [makeItem(), second]
        expect(derivePosition(third, items, queueItems, true)).toEqual({
            currentIndex: 0,
            queueIndex: -1,
            positionIndex: 1,
            positionTotal: 1,
        })
    })

    it("returns empty positions without an item", () => {
        const items = [makeItem(), second]
        expect(derivePosition(undefined, items, items, false)).toEqual({
            currentIndex: 0,
            queueIndex: -1,
            positionIndex: 1,
            positionTotal: 2,
        })
    })
})

describe("deriveResponsibilityStatus", () => {
    it("reports blocked for a task without a work item", () => {
        expect(deriveResponsibilityStatus(makeItem({ workItem: undefined }), "u1")).toBe(
            "blocked",
        )
    })

    it("reports assigned_to_me for a difference without a work item", () => {
        const difference = makeItem({
            identity: {
                itemType: "RECONCILIATION_DIFFERENCE",
                id: "diff-1",
                number: "RD-1",
                subjectHash: "h3",
            },
            workItem: undefined,
        })
        expect(deriveResponsibilityStatus(difference, undefined)).toBe("assigned_to_me")
    })

    it("derives status from work item state", () => {
        const base = makeItem()
        expect(
            deriveResponsibilityStatus(
                { ...base, workItem: { ...base.workItem!, status: "COMPLETED" } },
                "u1",
            ),
        ).toBe("completed")
        expect(
            deriveResponsibilityStatus(
                { ...base, workItem: { ...base.workItem!, status: "CLOSED" } },
                "u1",
            ),
        ).toBe("closed")
        expect(
            deriveResponsibilityStatus(
                { ...base, workItem: { ...base.workItem!, processingState: "APPROVAL_BLOCKED" } },
                "u1",
            ),
        ).toBe("blocked")
        expect(
            deriveResponsibilityStatus(
                { ...base, workItem: { ...base.workItem!, assignmentMode: "POOL", ownerUser: undefined } },
                "u1",
            ),
        ).toBe("pool_available")
    })

    it("compares the owner against the current user", () => {
        const base = makeItem()
        expect(deriveResponsibilityStatus(base, "u1")).toBe("assigned_to_me")
        expect(deriveResponsibilityStatus(base, "u2")).toBe("assigned_to_other")
        expect(deriveResponsibilityStatus(base, undefined)).toBe("assigned_to_other")
    })
})
