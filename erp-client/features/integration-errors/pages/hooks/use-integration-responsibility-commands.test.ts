import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import type { WorkItemResponsibilityCommand } from "@/features/work-items"
import { createCommandIdentityStore } from "../lib/command-identity"
import { makeItem } from "./test-fixtures"
import { useIntegrationResponsibilityCommands } from "./use-integration-responsibility-commands"

function renderCommands(overrides: Partial<{
    comment: string
    replacementTaskId: string
    item: ReturnType<typeof makeItem> | undefined
}> = {}) {
    const mutateAsync = vi.fn<(input: WorkItemResponsibilityCommand) => Promise<unknown>>()
    const refresh = vi.fn()
    const setLastResult = vi.fn()
    const setActionError = vi.fn()
    const afterResult = vi.fn()
    const commandIdentities = createCommandIdentityStore()

    const utils = renderHook(() =>
        useIntegrationResponsibilityCommands({
            item: overrides.item === undefined ? makeItem() : overrides.item,
            comment: overrides.comment ?? "",
            replacementTaskId: overrides.replacementTaskId ?? "",
            responsibilityMutation: { mutateAsync, isPending: false },
            commandIdentities,
            refresh,
            setLastResult,
            setActionError,
            afterResult,
        }),
    )

    return {
        ...utils,
        mutateAsync,
        refresh,
        setLastResult,
        setActionError,
        afterResult,
        commandIdentities,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useIntegrationResponsibilityCommands", () => {
    it("starts processing with a generated idempotency key and refreshes", async () => {
        const ctx = renderCommands()
        ctx.mutateAsync.mockResolvedValue({})

        await act(async () => {
            await ctx.result.current.handleStartProcessing()
        })

        expect(ctx.mutateAsync).toHaveBeenCalledWith({
            kind: "START_PROCESSING",
            workItemId: "wi_1",
            expectedTaskVersion: "5",
            idempotencyKey: expect.stringMatching(/^w29:start-processing:wi_1:/),
        })
        expect(ctx.refresh).toHaveBeenCalledTimes(1)
    })

    it("does not start processing without a work item", async () => {
        const ctx = renderCommands({ item: makeItem({ workItem: undefined }) })
        await act(async () => {
            await ctx.result.current.handleStartProcessing()
        })
        expect(ctx.mutateAsync).not.toHaveBeenCalled()
    })

    it("reports the error when starting fails", async () => {
        const ctx = renderCommands()
        ctx.mutateAsync.mockRejectedValue(new Error("网络中断"))
        await act(async () => {
            await ctx.result.current.handleStartProcessing()
        })
        expect(ctx.setActionError).toHaveBeenCalledWith("网络中断")
    })

    it("requires a comment before releasing to the team", async () => {
        const ctx = renderCommands({ comment: "" })
        await act(async () => {
            await ctx.result.current.handleReleaseToTeam()
        })
        expect(ctx.setActionError).toHaveBeenCalledWith("请先填写退回原因")
        expect(ctx.mutateAsync).not.toHaveBeenCalled()
    })

    it("releases to the team with the trimmed comment", async () => {
        const ctx = renderCommands({ comment: " 转给其他人 " })
        ctx.mutateAsync.mockResolvedValue({})
        await act(async () => {
            await ctx.result.current.handleReleaseToTeam()
        })
        expect(ctx.mutateAsync).toHaveBeenCalledWith({
            kind: "RELEASE_TO_TEAM",
            workItemId: "wi_1",
            expectedTaskVersion: "5",
            reason: "转给其他人",
            idempotencyKey: expect.stringMatching(/^w29:release-to-team:wi_1:/),
        })
        expect(ctx.setLastResult).toHaveBeenCalledWith(
            expect.objectContaining({ title: "已退回团队" }),
        )
        expect(ctx.refresh).toHaveBeenCalledTimes(1)
    })

    it("requires a replacement task before closing a duplicate", async () => {
        const ctx = renderCommands({ replacementTaskId: "" })
        await expect(
            act(async () => {
                await ctx.result.current.handleClose("CLOSE_DUPLICATE")
            }),
        ).rejects.toThrow("请先选择替代任务")
        expect(ctx.setActionError).toHaveBeenCalledWith("请先选择替代任务")
    })

    it("closes a duplicate with the replacement task", async () => {
        const ctx = renderCommands({ replacementTaskId: "rep-1" })
        ctx.mutateAsync.mockResolvedValue({})
        await act(async () => {
            await ctx.result.current.handleClose("CLOSE_DUPLICATE")
        })
        expect(ctx.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                kind: "CLOSE",
                reasonCode: "DUPLICATE",
                replacementWorkItemId: "rep-1",
            }),
        )
        expect(ctx.afterResult).toHaveBeenCalledWith(
            expect.objectContaining({
                title: "已关闭重复任务",
                terminal: true,
                stayOnItem: false,
            }),
        )
    })

    it("closes a misrouted task without a replacement", async () => {
        const ctx = renderCommands()
        ctx.mutateAsync.mockResolvedValue({})
        await act(async () => {
            await ctx.result.current.handleClose("CLOSE_MISROUTED")
        })
        expect(ctx.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                kind: "CLOSE",
                reasonCode: "MISROUTED",
                replacementWorkItemId: undefined,
            }),
        )
        expect(ctx.afterResult).toHaveBeenCalledWith(
            expect.objectContaining({ title: "已关闭误派" }),
        )
    })

    it("rethrows close failures after reporting them", async () => {
        const ctx = renderCommands()
        ctx.mutateAsync.mockRejectedValue(new Error("服务不可用"))
        await expect(
            act(async () => {
                await ctx.result.current.handleClose("CLOSE_MISROUTED")
            }),
        ).rejects.toThrow("服务不可用")
        expect(ctx.setActionError).toHaveBeenCalledWith("服务不可用")
    })
})
