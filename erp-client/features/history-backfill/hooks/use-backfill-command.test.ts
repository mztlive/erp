import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, waitFor } from '@testing-library/react'

import {
    submitHistoryBackfillCommand,
} from '@/features/history-backfill/api/history-backfill'
import { useBackfillCommand } from '@/features/history-backfill/hooks/use-backfill-command'
import { renderHookWithProviders } from '@/features/test-utils'
import {
    makeCommittedResult,
    makeJob,
    makeReport,
} from '@/features/history-backfill/test-fixtures'

vi.mock('@/features/history-backfill/api/history-backfill', () => ({
    fetchHistoryBackfillList: vi.fn(),
    fetchHistoryBackfillDetail: vi.fn(),
    submitHistoryBackfillCommand: vi.fn(),
}))

describe("useBackfillCommand", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    const job = makeJob()

    it("submits START with generated operationId/idempotencyKey and stores result", async () => {
        const committed = makeCommittedResult()
        vi.mocked(submitHistoryBackfillCommand).mockResolvedValue(committed)

        const { result } = renderHookWithProviders(() =>
            useBackfillCommand(job),
        )
        expect(result.current.actionResult).toBeNull()

        await act(async () => {
            await result.current.runCommand("START")
        })

        expect(submitHistoryBackfillCommand).toHaveBeenCalledWith({
            action: "START",
            jobId: job.id,
            expectedLockVersion: job.lockVersion,
            rangeStart: job.rangeStart,
            rangeEnd: job.rangeEnd,
            operationId: expect.stringMatching(/^op_/),
            idempotencyKey: expect.stringMatching(/^idem_start_/),
            itemIds: undefined,
            reportVersion: undefined,
        })
        expect(result.current.actionResult).toEqual(committed)
    })

    it("reuses the job idempotency namespace for RESUME", async () => {
        vi.mocked(submitHistoryBackfillCommand).mockResolvedValue(
            makeCommittedResult(),
        )

        const { result } = renderHookWithProviders(() =>
            useBackfillCommand(job),
        )

        await act(async () => {
            await result.current.runCommand("RESUME")
        })

        expect(submitHistoryBackfillCommand).toHaveBeenCalledWith(
            expect.objectContaining({
                action: "RESUME",
                idempotencyKey: `${job.idempotencyNamespace}:resume:${job.lockVersion}`,
            }),
        )
    })

    it("passes reportVersion and itemIds through when available", async () => {
        vi.mocked(submitHistoryBackfillCommand).mockResolvedValue(
            makeCommittedResult(),
        )
        const report = makeReport({ reportVersion: 7 })

        const { result } = renderHookWithProviders(() =>
            useBackfillCommand(
                { ...job, idempotencyNamespace: "mall-backfill:job-1" },
                report,
            ),
        )

        await act(async () => {
            await result.current.runCommand("REATTRIBUTE", ["item-1"])
        })

        expect(submitHistoryBackfillCommand).toHaveBeenCalledWith(
            expect.objectContaining({
                action: "REATTRIBUTE",
                reportVersion: 7,
                itemIds: ["item-1"],
            }),
        )
    })

    it("propagates mutation failures and keeps the previous result", async () => {
        vi.mocked(submitHistoryBackfillCommand).mockRejectedValue(
            new Error("提交失败"),
        )

        const { result } = renderHookWithProviders(() =>
            useBackfillCommand(job),
        )

        await act(async () => {
            await expect(result.current.runCommand("START")).rejects.toThrow(
                "提交失败",
            )
        })

        expect(result.current.actionResult).toBeNull()
    })

    it("reflects pending state while a command is in flight", async () => {
        let resolveCommand: (value: typeof committedResult) => void
        const committedResult = makeCommittedResult()
        vi.mocked(submitHistoryBackfillCommand).mockImplementation(
            () =>
                new Promise((resolve) => {
                    resolveCommand = resolve
                }),
        )

        const { result } = renderHookWithProviders(() =>
            useBackfillCommand(job),
        )
        expect(result.current.isPending).toBe(false)

        let pending: Promise<unknown>
        act(() => {
            pending = result.current.runCommand("START")
        })

        await waitFor(() => expect(result.current.isPending).toBe(true))

        await act(async () => {
            resolveCommand!(committedResult)
            await pending!
        })
        expect(result.current.isPending).toBe(false)
        expect(result.current.actionResult).toEqual(committedResult)
    })
})
