import { beforeEach, describe, expect, it, vi } from "vitest"
import { act } from "@testing-library/react"

import { executeImportCommand } from "@/features/import-opening/api/legacy-import"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useExecutionActions } from "@/features/import-opening/hooks/use-execution-actions"
import type {
    ImportBatchView,
    ImportExecutionResult,
} from "@/features/import-opening/types"
import { renderHookWithProviders } from "@/features/test-utils"

vi.mock("@/features/import-opening/api/legacy-import", () => ({
    fetchImportBatchList: vi.fn(),
    fetchImportBatchDetail: vi.fn(),
    fetchImportIssues: vi.fn(),
    startImportConfirmationProcessing: vi.fn(),
    completeImportConfirmation: vi.fn(),
    executeImportCommand: vi.fn(),
}))

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: vi.fn(),
}))

function makeBatch(
    overrides?: Partial<ImportBatchView>,
): ImportBatchView {
    return {
        batchId: "b1",
        batchNo: "B-001",
        environment: "VALIDATION",
        sourceSystem: { id: "sys-a", name: "sys-a" },
        sourceObjectSet: ["CUSTOMER"],
        baselineDate: "2025-01-01",
        importRuleVersion: "v3",
        trialVersion: "3",
        stage: "CONFIRM",
        status: "READY_TO_APPLY",
        formalDataFormed: false,
        notFormalDataMessage: "",
        resultAssets: [],
        metrics: { total: 0, valid: 0, conflict: 0, failed: 0, skipped: 0 },
        confirmations: [],
        productionGates: {
            validationEnvPassed: true,
            allConfirmationsComplete: true,
            noBlockingIssues: true,
            trialVersionMatches: true,
            ruleVersionStable: true,
            workItemTypeRegistered: true,
        },
        openingPolicyHints: [],
        allowedActions: ["START_APPLY", "CANCEL_PENDING", "RETRY_FAILED"],
        actionBlockers: [],
        version: "7",
        updatedAt: "2025-01-01T00:00:00.000Z",
        initiatorLabel: "—",
        ...overrides,
    }
}

const execResult: ImportExecutionResult = {
    action: "START_APPLY",
    resultStatus: "STARTED",
    batchId: "b1",
    batchStatus: "APPLYING",
    batchVersion: "1",
    backgroundJobId: "job1",
    backgroundJobStatus: "running",
    backgroundJobVersion: "1",
    affectedItems: 5,
    nextStep: "MONITOR_PROGRESS",
    auditReceipt: "r1",
}

function grantPermission() {
    vi.mocked(useAccountProfileQuery).mockReturnValue({
        data: { permissions: ["legacy_import_batch:execute"] },
    } as never)
}

beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(useAccountProfileQuery).mockReturnValue({
        data: undefined,
    } as never)
})

describe("useExecutionActions", () => {
    it("hides all actions without the execute permission", () => {
        const batch = makeBatch()
        const onGoSection = vi.fn()
        const { result } = renderHookWithProviders(() =>
            useExecutionActions(batch, onGoSection),
        )
        expect(result.current.visible).toBe(false)
        expect(result.current.canStart).toBe(false)
        expect(result.current.canCancel).toBe(false)
        expect(result.current.canRetry).toBe(false)
    })

    it("derives available actions from permissions and allowedActions", () => {
        grantPermission()
        const { result } = renderHookWithProviders(() =>
            useExecutionActions(makeBatch(), vi.fn()),
        )
        expect(result.current.visible).toBe(true)
        expect(result.current.canStart).toBe(true)
        expect(result.current.canCancel).toBe(true)
        expect(result.current.canRetry).toBe(true)
    })

    it("disables actions not allowed on the batch", () => {
        grantPermission()
        const { result } = renderHookWithProviders(() =>
            useExecutionActions(
                makeBatch({ allowedActions: ["CANCEL_PENDING"] }),
                vi.fn(),
            ),
        )
        expect(result.current.canStart).toBe(false)
        expect(result.current.canCancel).toBe(true)
        expect(result.current.canRetry).toBe(false)
    })

    it("executes a command with version guard and a w18: request id", async () => {
        grantPermission()
        vi.mocked(executeImportCommand).mockResolvedValue(execResult)
        const onGoSection = vi.fn()
        const batch = makeBatch()
        const { result } = renderHookWithProviders(() =>
            useExecutionActions(batch, onGoSection),
        )

        await act(async () => {
            await result.current.execute("START_APPLY")
        })

        expect(executeImportCommand).toHaveBeenCalledWith({
            batchId: "b1",
            expectedBatchVersion: "7",
            expectedTrialVersion: "3",
            action: "START_APPLY",
            reasonCode: undefined,
            comment: undefined,
            requestId: expect.stringMatching(/^w18:/),
        })
        expect(onGoSection).toHaveBeenCalledWith("progress")
    })

    it("omits the trial version guard when the batch has none", async () => {
        grantPermission()
        vi.mocked(executeImportCommand).mockResolvedValue(execResult)
        const { result } = renderHookWithProviders(() =>
            useExecutionActions(makeBatch({ trialVersion: "0" }), vi.fn()),
        )

        await act(async () => {
            await result.current.execute("START_APPLY")
        })

        expect(executeImportCommand).toHaveBeenCalledWith(
            expect.objectContaining({ expectedTrialVersion: undefined }),
        )
    })

    it("trims the comment and forwards the cancel reason", async () => {
        grantPermission()
        vi.mocked(executeImportCommand).mockResolvedValue({
            ...execResult,
            action: "CANCEL_PENDING",
            resultStatus: "CANCELLED",
        })
        const { result } = renderHookWithProviders(() =>
            useExecutionActions(makeBatch(), vi.fn()),
        )

        await act(async () => {
            await result.current.execute(
                "CANCEL_PENDING",
                "BUSINESS_WINDOW_CLOSED",
                "  窗口已关  ",
            )
        })

        expect(executeImportCommand).toHaveBeenCalledWith(
            expect.objectContaining({
                action: "CANCEL_PENDING",
                reasonCode: "BUSINESS_WINDOW_CLOSED",
                comment: "窗口已关",
            }),
        )
    })

    it.each([
        ["MONITOR_PROGRESS", "progress"],
        ["REVIEW_RESULT", "result"],
        ["START_APPLY", "confirm"],
    ] as const)("routes nextStep=%s to the %s section", async (nextStep, section) => {
        grantPermission()
        vi.mocked(executeImportCommand).mockResolvedValue({
            ...execResult,
            nextStep,
        })
        const onGoSection = vi.fn()
        const { result } = renderHookWithProviders(() =>
            useExecutionActions(makeBatch(), onGoSection),
        )

        await act(async () => {
            await result.current.execute("START_APPLY")
        })

        expect(onGoSection).toHaveBeenCalledWith(section)
    })

    it("closes open dialogs after a successful execution", async () => {
        grantPermission()
        vi.mocked(executeImportCommand).mockResolvedValue(execResult)
        const { result } = renderHookWithProviders(() =>
            useExecutionActions(makeBatch(), vi.fn()),
        )
        act(() => result.current.setConfirming("START_APPLY"))
        act(() => result.current.setCancelling(true))
        expect(result.current.confirming).toBe("START_APPLY")
        expect(result.current.cancelling).toBe(true)

        await act(async () => {
            await result.current.execute("START_APPLY")
        })

        expect(result.current.confirming).toBeUndefined()
        expect(result.current.cancelling).toBe(false)
    })
})
