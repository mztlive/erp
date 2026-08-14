import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { actionReasonSchema, useTaskAction } from "./use-task-action"
import { makeQueueItem } from "./test-fixtures"

const workItemsMocks = vi.hoisted(() => ({
    useWorkItemResponsibilityMutation: vi.fn(),
}))

vi.mock("@/features/work-items", () => ({
    useWorkItemResponsibilityMutation:
        workItemsMocks.useWorkItemResponsibilityMutation,
}))

const mutateAsync = vi.fn()

function setMutationState(overrides: Record<string, unknown> = {}) {
    workItemsMocks.useWorkItemResponsibilityMutation.mockReturnValue({
        mutateAsync,
        isPending: false,
        isError: false,
        error: null,
        ...overrides,
    })
}

describe("useTaskAction", () => {
    beforeEach(() => {
        mutateAsync.mockReset().mockResolvedValue(undefined)
        workItemsMocks.useWorkItemResponsibilityMutation.mockReset()
        setMutationState()
    })

    it("starts processing with the server task version and a fresh idempotency key", async () => {
        const item = makeQueueItem()
        const { result } = renderHook(() => useTaskAction(item))

        await act(async () => {
            await result.current.runResponsibilityAction("START_PROCESSING")
        })

        expect(mutateAsync).toHaveBeenCalledTimes(1)
        const [command] = mutateAsync.mock.calls[0]
        expect(command).toMatchObject({
            workItemId: item.workItemId,
            expectedTaskVersion: item.taskVersion,
            kind: "START_PROCESSING",
        })
        expect(command.idempotencyKey).toMatch(
            /^work-item-responsibility:wi-1:START_PROCESSING:/,
        )
        expect(result.current.action).toBeNull()
    })

    it("reuses the same idempotency key after an indeterminate failure", async () => {
        const item = makeQueueItem()
        mutateAsync.mockRejectedValueOnce({ kind: "Network", message: "中断" })
        const { result } = renderHook(() => useTaskAction(item))

        await act(async () => {
            await result.current.runResponsibilityAction("START_PROCESSING")
        })
        await act(async () => {
            await result.current.runResponsibilityAction("START_PROCESSING")
        })

        expect(mutateAsync).toHaveBeenCalledTimes(2)
        const first = mutateAsync.mock.calls[0][0] as {
            idempotencyKey: string
        }
        const second = mutateAsync.mock.calls[1][0] as {
            idempotencyKey: string
        }
        expect(second.idempotencyKey).toBe(first.idempotencyKey)
    })

    it("rotates the idempotency key after a determined failure", async () => {
        const item = makeQueueItem()
        mutateAsync.mockRejectedValueOnce({
            kind: "Http",
            message: "数据已更新",
            status: 409,
        })
        const { result } = renderHook(() => useTaskAction(item))

        await act(async () => {
            await result.current.runResponsibilityAction("START_PROCESSING")
        })
        await act(async () => {
            await result.current.runResponsibilityAction("START_PROCESSING")
        })

        expect(mutateAsync).toHaveBeenCalledTimes(2)
        const first = mutateAsync.mock.calls[0][0] as {
            idempotencyKey: string
        }
        const second = mutateAsync.mock.calls[1][0] as {
            idempotencyKey: string
        }
        expect(second.idempotencyKey).not.toBe(first.idempotencyKey)
    })

    it("carries the chosen target and reason when reassigning", async () => {
        const item = makeQueueItem()
        const { result } = renderHook(() => useTaskAction(item))

        await act(async () => {
            await result.current.runResponsibilityAction(
                "REASSIGN",
                "转给小李",
                "user-2",
            )
        })

        const [command] = mutateAsync.mock.calls[0]
        expect(command).toMatchObject({
            workItemId: item.workItemId,
            expectedTaskVersion: item.taskVersion,
            kind: "REASSIGN",
            targetUserId: "user-2",
            reason: "转给小李",
        })
    })

    it("closes as duplicate with a replacement, and as misrouted without one", async () => {
        const item = makeQueueItem()
        const { result } = renderHook(() => useTaskAction(item))

        await act(async () => {
            await result.current.runResponsibilityAction(
                "CLOSE",
                "重复了",
                "",
                "DUPLICATE",
                "wi-2",
            )
        })
        await act(async () => {
            await result.current.runResponsibilityAction(
                "CLOSE",
                "派错了",
                "",
                "MISROUTED",
            )
        })

        const [duplicateCommand] = mutateAsync.mock.calls[0]
        const [misroutedCommand] = mutateAsync.mock.calls[1]
        expect(duplicateCommand).toMatchObject({
            kind: "CLOSE",
            reasonCode: "DUPLICATE",
            replacementWorkItemId: "wi-2",
            comment: "重复了",
        })
        expect(misroutedCommand).toMatchObject({
            kind: "CLOSE",
            reasonCode: "MISROUTED",
            replacementWorkItemId: undefined,
            comment: "派错了",
        })
    })

    it("does nothing without a selected item", async () => {
        const { result } = renderHook(() => useTaskAction(undefined))

        await act(async () => {
            await result.current.runResponsibilityAction("START_PROCESSING")
        })

        expect(mutateAsync).not.toHaveBeenCalled()
    })

    it("resets the action form after a successful command", async () => {
        const item = makeQueueItem()
        const { result } = renderHook(() => useTaskAction(item))

        act(() => {
            result.current.setAction("RELEASE_TO_TEAM")
            result.current.actionForm.setFieldValue("reason", "原因")
        })

        await act(async () => {
            await result.current.runResponsibilityAction(
                "RELEASE_TO_TEAM",
                "原因",
            )
        })

        expect(result.current.action).toBeNull()
        expect(result.current.actionForm.getFieldValue("reason")).toBe("")
    })

    it("exposes the mutation state to the caller", () => {
        setMutationState({
            isPending: true,
            isError: true,
            error: new Error("boom"),
        })
        const { result } = renderHook(() => useTaskAction(makeQueueItem()))

        expect(result.current.isPending).toBe(true)
        expect(result.current.isError).toBe(true)
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("actionReasonSchema", () => {
    const base = {
        targetUserId: "",
        reasonCode: "MISROUTED" as const,
        replacementWorkItemId: "",
    }

    it("rejects an empty reason", () => {
        const parsed = actionReasonSchema.safeParse({ ...base, reason: "  " })
        expect(parsed.success).toBe(false)
    })

    it("rejects DUPLICATE without a replacement work item", () => {
        const parsed = actionReasonSchema.safeParse({
            ...base,
            reason: "重复",
            reasonCode: "DUPLICATE",
            replacementWorkItemId: "  ",
        })
        expect(parsed.success).toBe(false)
    })

    it("accepts a duplicate close with a replacement work item", () => {
        const parsed = actionReasonSchema.safeParse({
            ...base,
            reason: "重复",
            reasonCode: "DUPLICATE",
            replacementWorkItemId: "wi-2",
        })
        expect(parsed.success).toBe(true)
    })
})
