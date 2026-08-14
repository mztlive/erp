import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import type {
    DirectReconciliationInput,
    IntegrationActionKind,
    IntegrationFormalResult,
    IntegrationResolutionItemView,
} from "../../types"
import { createCommandIdentityStore } from "../lib/command-identity"
import { makeItem, makeResult } from "./test-fixtures"
import { useIntegrationDirectActions } from "./use-integration-direct-actions"

const differenceItem = (overrides: Partial<IntegrationResolutionItemView> = {}) =>
    makeItem({
        identity: {
            itemType: "RECONCILIATION_DIFFERENCE",
            id: "diff-1",
            number: "RD-1",
            subjectHash: "h3",
        },
        hasWorkItem: false,
        workItem: undefined,
        allowedActions: [
            "CONFIRM_NO_ERROR",
            "CONFIRM_VALID_DIFFERENCE",
            "QUERY_ORIGINAL_RESULT",
            "LINK_COMPENSATION",
            "ADD_EVIDENCE",
        ],
        reconciliationReasonRegistry: {
            reasonRegistryId: "reg1",
            reasonRegistryVersion: 1,
            registeredReasons: [
                {
                    registeredReasonId: "BUSINESS_CONFIRMED_NO_ERROR",
                    registeredReasonVersion: 1,
                    conclusion: "CONFIRM_NO_ERROR",
                    label: "业务确认无误",
                    requiredEvidenceKinds: [],
                },
                {
                    registeredReasonId: "COMPENSATION_CLOSED",
                    registeredReasonVersion: 1,
                    conclusion: "CONFIRM_VALID_DIFFERENCE",
                    label: "补偿已关闭",
                    requiredEvidenceKinds: [],
                },
            ],
        },
        ...overrides,
    })

function renderDirectActions(overrides: Partial<{
    item: IntegrationResolutionItemView | undefined
    reconReasonId: string
    can: (action: IntegrationActionKind) => boolean
}> = {}) {
    const mutateAsync = vi.fn<
        (input: DirectReconciliationInput) => Promise<IntegrationFormalResult>
    >()
    const afterResult = vi.fn()
    const setActionError = vi.fn()
    const commandIdentities = createCommandIdentityStore()
    const item = overrides.item === undefined ? differenceItem() : overrides.item
    const can = overrides.can ?? ((action: IntegrationActionKind) =>
        Boolean(item?.allowedActions.includes(action)))

    const utils = renderHook(() =>
        useIntegrationDirectActions({
            item,
            can,
            reconReasonId: overrides.reconReasonId ?? "BUSINESS_CONFIRMED_NO_ERROR",
            comment: "",
            commandIdentities,
            directMutation: { mutateAsync, isPending: false },
            afterResult,
            setActionError,
        }),
    )

    return { ...utils, mutateAsync, afterResult, setActionError }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useIntegrationDirectActions", () => {
    it("rejects a terminal conclusion when the selected reason mismatches", async () => {
        const ctx = renderDirectActions({ reconReasonId: "COMPENSATION_CLOSED" })
        await act(async () => {
            await ctx.result.current.handleDirectTerminal("CONFIRM_NO_ERROR")
        })
        expect(ctx.setActionError).toHaveBeenCalledWith(
            "请选择与结论匹配的注册原因",
        )
        expect(ctx.mutateAsync).not.toHaveBeenCalled()
    })

    it("submits the terminal conclusion for a matching reason", async () => {
        const ctx = renderDirectActions()
        ctx.mutateAsync.mockResolvedValue(
            makeResult({ terminal: true, stayOnItem: false }),
        )
        await act(async () => {
            await ctx.result.current.handleDirectTerminal("CONFIRM_NO_ERROR")
        })
        expect(ctx.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                differenceId: "diff-1",
                expectedDifferenceVersion: "v1",
                decision: expect.objectContaining({
                    kind: "TERMINAL_CONCLUSION",
                    reasonCode: "BUSINESS_CONFIRMED_NO_ERROR",
                    reasonRegistryId: "reg1",
                    conclusion: "CONFIRM_NO_ERROR",
                }),
            }),
        )
        expect(ctx.afterResult).toHaveBeenCalledTimes(1)
    })

    it("blocks evidence actions without linked evidence", async () => {
        const ctx = renderDirectActions()
        await act(async () => {
            await ctx.result.current.handleDirectAction("LINK_COMPENSATION")
        })
        expect(ctx.setActionError).toHaveBeenCalledWith(
            "请先从受控证据入口关联已有证据",
        )
        expect(ctx.mutateAsync).not.toHaveBeenCalled()
    })

    it("submits a non-terminal direct action with the linked evidence", async () => {
        const item = differenceItem({
            linkedEvidence: [
                { kind: "COMPENSATION_RESULT", recordId: "c1", label: "补偿记录" },
            ],
        })
        const ctx = renderDirectActions({ item })
        ctx.mutateAsync.mockResolvedValue(makeResult())
        await act(async () => {
            await ctx.result.current.handleDirectAction("LINK_COMPENSATION")
        })
        expect(ctx.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                differenceId: "diff-1",
                decision: expect.objectContaining({
                    kind: "NON_TERMINAL_ACTION",
                    action: "LINK_COMPENSATION",
                    evidenceRefs: [
                        {
                            kind: "COMPENSATION_RESULT",
                            recordId: "c1",
                            label: "补偿记录",
                        },
                    ],
                }),
            }),
        )
        expect(ctx.afterResult).toHaveBeenCalledTimes(1)
    })

    it("does nothing for disallowed actions", async () => {
        const item = differenceItem()
        const ctx = renderDirectActions({
            item,
            can: () => false,
        })
        await act(async () => {
            await ctx.result.current.handleDirectAction("QUERY_ORIGINAL_RESULT")
        })
        expect(ctx.mutateAsync).not.toHaveBeenCalled()
    })

    it("does nothing for terminal conclusions on tasks with work items", async () => {
        const item = differenceItem({ hasWorkItem: true })
        const ctx = renderDirectActions({ item })
        await act(async () => {
            await ctx.result.current.handleDirectTerminal("CONFIRM_NO_ERROR")
        })
        expect(ctx.mutateAsync).not.toHaveBeenCalled()
    })
})
