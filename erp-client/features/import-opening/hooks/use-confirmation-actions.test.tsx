import { beforeEach, describe, expect, it, vi } from "vitest"
import { act } from "@testing-library/react"

import { completeImportConfirmation } from "@/features/import-opening/api/legacy-import"
import { useConfirmationActions } from "@/features/import-opening/hooks/use-confirmation-actions"
import type {
    ImportBatchView,
    ImportConfirmationView,
} from "@/features/import-opening/types"
import { renderHookWithProviders } from "@/features/test-utils"

vi.mock("@/features/import-opening/api/legacy-import", () => ({
    fetchImportBatchList: vi.fn(),
    fetchImportBatchDetail: vi.fn(),
    fetchImportIssues: vi.fn(),
    completeImportConfirmation: vi.fn(),
    executeImportCommand: vi.fn(),
}))

function makeConfirmation(
    overrides?: Partial<ImportConfirmationView>,
): ImportConfirmationView {
    return {
        confirmationId: "c1",
        scope: "SALES",
        result: "PENDING",
        trialVersion: "3",
        inViewerResponsibility: true,
        focused: false,
        workItem: {
            workItemId: "w1",
            taskVersion: "v1",
            subjectVersion: "s1",
            status: "OPEN",
            processingState: "READY",
            allowedActions: ["PROCESS", "CONFIRM_SCOPE", "RETURN_FOR_FIX"],
            actionBlockers: [],
        },
        ...overrides,
    }
}

function makeBatch(overrides?: Partial<ImportBatchView>): ImportBatchView {
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
        status: "AWAITING_CONFIRMATION",
        formalDataFormed: false,
        notFormalDataMessage: "",
        resultAssets: [],
        metrics: { total: 0, valid: 0, conflict: 0, failed: 0, skipped: 0 },
        confirmations: [],
        productionGates: {
            validationEnvPassed: true,
            allConfirmationsComplete: false,
            noBlockingIssues: true,
            trialVersionMatches: true,
            ruleVersionStable: true,
            workItemTypeRegistered: true,
        },
        openingPolicyHints: [],
        allowedActions: [],
        actionBlockers: [],
        version: "7",
        updatedAt: "2025-01-01T00:00:00.000Z",
        initiatorLabel: "—",
        ...overrides,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useConfirmationActions", () => {
    it("completes a confirmation with the full command payload", async () => {
        vi.mocked(completeImportConfirmation).mockResolvedValue(undefined)
        const confirmation = makeConfirmation()
        const batch = makeBatch({ confirmations: [confirmation] })
        const { result } = renderHookWithProviders(() =>
            useConfirmationActions(batch),
        )

        await act(async () => {
            await result.current.complete(
                confirmation,
                "RETURN_FOR_FIX",
                "DATA_MISMATCH",
                "数据对不上",
            )
        })

        expect(completeImportConfirmation).toHaveBeenCalledWith({
            batchId: "b1",
            batchVersion: "7",
            trialVersion: "3",
            confirmationScope: "SALES",
            workItemId: "w1",
            taskVersion: "v1",
            subjectVersion: "s1",
            action: "RETURN_FOR_FIX",
            reasonCode: "DATA_MISMATCH",
            comment: "数据对不上",
            idempotencyKey: expect.stringMatching(/^w18:/),
        })
    })

    it("toggles the confirm/return dialogs via setters", () => {
        const confirmation = makeConfirmation()
        const { result } = renderHookWithProviders(() =>
            useConfirmationActions(
                makeBatch({ confirmations: [confirmation] }),
            ),
        )
        expect(result.current.confirming).toBeUndefined()
        expect(result.current.returning).toBeUndefined()

        act(() => result.current.setConfirming(confirmation))
        expect(result.current.confirming).toBe(confirmation)

        act(() => result.current.setReturning(confirmation))
        expect(result.current.returning).toBe(confirmation)

        act(() => result.current.setConfirming(undefined))
        expect(result.current.confirming).toBeUndefined()
    })
})
