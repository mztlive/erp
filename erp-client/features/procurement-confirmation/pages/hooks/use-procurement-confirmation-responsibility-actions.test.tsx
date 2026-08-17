import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, cleanup } from "@testing-library/react"
import * as React from "react"

import {
    useProcurementResponsibilityActions,
    type ProcurementResponsibilityActionsOptions,
} from "./use-procurement-confirmation-responsibility-actions"
import type { FormalOutcome } from "@/features/procurement-confirmation/types"
import { makeTask } from "./test-data"

type ResultState =
    import("@/components/business/feedback").ResultState<FormalOutcome>

type ResponsibilityMutation = ReturnType<
    typeof import("@/features/work-items").useWorkItemResponsibilityMutation
>

function makeResponsibilityMutation() {
    const mutateAsync = vi.fn(async (_cmd: unknown) => ({
        id: "wi_1",
        status: "OPEN",
        task_version: 6,
    }))
    return { mutateAsync } as unknown as ResponsibilityMutation & {
        mutateAsync: typeof mutateAsync
    }
}

function renderResponsibility(
    overrides: Partial<ProcurementResponsibilityActionsOptions> = {},
) {
    const task = overrides.task ?? makeTask()
    const state = {
        actionError: null as string | null,
        lastResult: null as ResultState,
    }
    const callbacks = {
        neighborId: vi.fn((delta: number) =>
            delta === 1 ? "wi_2" : undefined,
        ),
        goToWorkItem: vi.fn(),
        replaceUrl: vi.fn(),
        handleSave: vi.fn(async () => true),
        assertAllowed: vi.fn(),
        queueRefetch: vi.fn(async () => ({
            data: { tasks: [task] },
            isError: false,
            error: null,
        })),
    }
    const setters = {
        setActionError: vi.fn((next: string | null) => {
            state.actionError = next
        }) as unknown as React.Dispatch<React.SetStateAction<string | null>>,
        setLastResult: vi.fn((next: ResultState) => {
            state.lastResult = next
        }) as unknown as React.Dispatch<React.SetStateAction<ResultState>>,
    }
    const mutation = makeResponsibilityMutation()
    const utils = renderHook(() =>
        useProcurementResponsibilityActions({
            task: overrides.task ?? task,
            dirty: overrides.dirty ?? false,
            handleSave: callbacks.handleSave,
            responsibilityMutation: mutation,
            queueRefetch: callbacks.queueRefetch,
            replaceUrl: callbacks.replaceUrl,
            neighborId: callbacks.neighborId,
            goToWorkItem: callbacks.goToWorkItem,
            assertAllowed: callbacks.assertAllowed,
            ...setters,
        }),
    )
    return { ...utils, state, callbacks, setters, mutation, task }
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("useProcurementResponsibilityActions", () => {
    it("releases to team after saving the dirty draft", async () => {
        const { result, state, callbacks, mutation } = renderResponsibility({
            dirty: true,
        })
        await act(async () => {
            await result.current.handleReleaseToTeam()
        })
        expect(callbacks.handleSave).toHaveBeenCalledTimes(1)
        expect(callbacks.queueRefetch).toHaveBeenCalledTimes(1)
        expect(mutation.mutateAsync).toHaveBeenCalledWith({
            kind: "RELEASE_TO_TEAM",
            workItemId: "wi_1",
            expectedTaskVersion: "5",
            reason: "当前确认数据已保存，退回团队继续安排",
            idempotencyKey: "w07:wi_1:5:release",
        })
        expect(state.lastResult).toMatchObject({
            status: "blocked",
            title: "当前项已退回团队",
        })
        expect(callbacks.goToWorkItem).toHaveBeenCalledWith("wi_2")
    })

    it("skips saving when the draft is clean", async () => {
        const { result, callbacks } = renderResponsibility()
        await act(async () => {
            await result.current.handleReleaseToTeam()
        })
        expect(callbacks.handleSave).not.toHaveBeenCalled()
        expect(callbacks.queueRefetch).not.toHaveBeenCalled()
        expect(callbacks.goToWorkItem).toHaveBeenCalledWith("wi_2")
    })

    it("aborts releasing when saving the dirty draft fails", async () => {
        const { result, state, callbacks } = renderResponsibility({
            dirty: true,
        })
        callbacks.handleSave.mockResolvedValueOnce(false)
        await act(async () => {
            await result.current.handleReleaseToTeam()
        })
        expect(state.actionError).toBe(
            "有未保存的确认分行修改且保存失败；请重试保存后再跳过",
        )
        expect(callbacks.goToWorkItem).not.toHaveBeenCalled()
    })

    it("reports when the refreshed task is missing after saving", async () => {
        const { result, state, callbacks } = renderResponsibility({
            dirty: true,
        })
        callbacks.queueRefetch.mockResolvedValueOnce({
            data: { tasks: [] },
            isError: false,
            error: null,
        })
        await act(async () => {
            await result.current.handleReleaseToTeam()
        })
        expect(state.actionError).toBe(
            "保存后未取得当前任务的新版本，已禁止使用旧版本退回团队",
        )
    })

    it("rejects releasing a task that lost the action", async () => {
        const { result, state } = renderResponsibility({
            task: makeTask({ allowedActions: ["SAVE"] }),
        })
        await act(async () => {
            await result.current.handleReleaseToTeam()
        })
        expect(state.actionError).toBe("当前责任已变化，请刷新后再退回团队")
    })

    it("reports when releasing leaves the task not open", async () => {
        const { result, state, mutation } = renderResponsibility()
        mutation.mutateAsync.mockResolvedValueOnce({
            id: "wi_1",
            status: "COMPLETED",
            task_version: 6,
        })
        await act(async () => {
            await result.current.handleReleaseToTeam()
        })
        expect(state.actionError).toBe("退回团队后任务未保持开放，请刷新核对")
        expect(state.lastResult).toBeNull()
    })

    it("starts processing and rewrites the URL scope", async () => {
        const { result, callbacks, mutation } = renderResponsibility()
        await act(async () => {
            await result.current.handleStartProcessing()
        })
        expect(callbacks.assertAllowed).toHaveBeenCalledWith("START_PROCESSING")
        expect(mutation.mutateAsync).toHaveBeenCalledWith({
            kind: "START_PROCESSING",
            workItemId: "wi_1",
            expectedTaskVersion: "5",
            idempotencyKey: "w07:wi_1:5:start",
        })
        expect(callbacks.replaceUrl).toHaveBeenCalledWith({
            scope: null,
            queueContextId: null,
            currentWorkItemId: "wi_1",
        })
        expect(callbacks.queueRefetch).toHaveBeenCalledTimes(1)
    })

    it("reports denied start processing attempts", async () => {
        const { result, state, callbacks } = renderResponsibility()
        callbacks.assertAllowed.mockImplementation(() => {
            throw new Error("当前责任或任务版本已变化，请刷新后再处理")
        })
        await act(async () => {
            await result.current.handleStartProcessing()
        })
        expect(state.actionError).toBe(
            "当前责任或任务版本已变化，请刷新后再处理",
        )
    })
})
