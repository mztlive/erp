import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import type {
    IntegrationFormalResult,
    IntegrationResolutionItemView,
} from "../../types"
import { makeItem, makeResult } from "./test-fixtures"
import { useIntegrationActions } from "./use-integration-actions"

const mocks = vi.hoisted(() => ({
    responsibility: { mutateAsync: vi.fn(), isPending: false },
    action: { mutateAsync: vi.fn(), isPending: false },
    resolve: { mutateAsync: vi.fn(), isPending: false },
    direct: { mutateAsync: vi.fn(), isPending: false },
}))

vi.mock("@/features/work-items", () => ({
    useWorkItemResponsibilityMutation: () => mocks.responsibility,
}))

vi.mock("../../hooks/queries", () => ({
    useIntegrationQueueQuery: () => ({ data: undefined, isPending: true }),
    useIntegrationItemQuery: () => ({ data: undefined, isPending: true }),
    useIntegrationActionMutation: () => mocks.action,
    useResolveIntegrationMutation: () => mocks.resolve,
    useDirectReconciliationMutation: () => mocks.direct,
}))

type ActionProps = Parameters<typeof useIntegrationActions>[0]

function makeProps(overrides: Partial<ActionProps> = {}): ActionProps {
    return {
        item: makeItem(),
        focusMode: false,
        autoNext: false,
        lastResult: null,
        setLastResult:
            vi.fn<(result: IntegrationFormalResult | null) => void>(),
        setActionError: vi.fn<(error: string | null) => void>(),
        userId: "u1",
        refetch: vi.fn<() => void>(),
        goToItem:
            vi.fn<
                (next: IntegrationResolutionItemView | null | undefined) => void
            >(),
        neighbor:
            vi.fn<(delta: number) => IntegrationResolutionItemView | null>(),
        ...overrides,
    }
}

function renderActions(props: ActionProps) {
    return renderHook((p: ActionProps) => useIntegrationActions(p), {
        initialProps: props,
    })
}

const differenceItem = (
    overrides: Partial<IntegrationResolutionItemView> = {},
) =>
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

beforeEach(() => {
    vi.useFakeTimers()
    mocks.responsibility.mutateAsync.mockReset()
    mocks.action.mutateAsync.mockReset()
    mocks.resolve.mutateAsync.mockReset()
    mocks.direct.mutateAsync.mockReset()
    mocks.responsibility.isPending = false
    mocks.action.isPending = false
    mocks.resolve.isPending = false
    mocks.direct.isPending = false
})

afterEach(() => {
    vi.useRealTimers()
})

describe("useIntegrationActions — responsibilityStatus", () => {
    it("reports blocked for a task without a work item", () => {
        const { result } = renderActions(
            makeProps({ item: makeItem({ workItem: undefined }) }),
        )
        expect(result.current.responsibilityStatus).toBe("blocked")
    })

    it("reports assigned_to_me for a difference without a work item", () => {
        const { result } = renderActions(makeProps({ item: differenceItem() }))
        expect(result.current.responsibilityStatus).toBe("assigned_to_me")
    })

    it("derives status from work item state", () => {
        const completed = renderActions(
            makeProps({
                item: makeItem({
                    workItem: { ...makeItem().workItem!, status: "COMPLETED" },
                }),
            }),
        )
        expect(completed.result.current.responsibilityStatus).toBe("completed")

        const closed = renderActions(
            makeProps({
                item: makeItem({
                    workItem: { ...makeItem().workItem!, status: "CLOSED" },
                }),
            }),
        )
        expect(closed.result.current.responsibilityStatus).toBe("closed")

        const blocked = renderActions(
            makeProps({
                item: makeItem({
                    workItem: {
                        ...makeItem().workItem!,
                        processingState: "APPROVAL_BLOCKED",
                    },
                }),
            }),
        )
        expect(blocked.result.current.responsibilityStatus).toBe("blocked")

        const missingOwner = renderActions(
            makeProps({
                item: makeItem({
                    workItem: {
                        ...makeItem().workItem!,
                        ownerUser: undefined,
                    },
                }),
            }),
        )
        expect(missingOwner.result.current.responsibilityStatus).toBe(
            "assigned_to_other",
        )

        const other = renderActions(makeProps({ userId: "u2" }))
        expect(other.result.current.responsibilityStatus).toBe(
            "assigned_to_other",
        )
    })
})

describe("useIntegrationActions — task actions", () => {
    it("blocks actions when not assigned to the current user", async () => {
        const props = makeProps({ userId: "u2" })
        const { result } = renderActions(props)
        await act(async () => {
            await result.current.runTaskAction("QUERY_ORIGINAL_RESULT")
        })
        expect(mocks.action.mutateAsync).not.toHaveBeenCalled()
    })

    it("submits a task action with command identities and reports the result", async () => {
        const resultValue = makeResult({ status: "unknown" })
        mocks.action.mutateAsync.mockResolvedValue(resultValue)
        const props = makeProps()
        const { result } = renderActions(props)

        await act(async () => {
            await result.current.runTaskAction("QUERY_ORIGINAL_RESULT")
        })

        expect(mocks.action.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                itemType: "ERROR_TASK",
                itemId: "task-1",
                workItemId: "wi_1",
                expectedSubjectVersion: "v3",
                expectedTaskVersion: "5",
                kind: "QUERY_ORIGINAL_RESULT",
                operationId: expect.stringMatching(
                    /^w29:QUERY_ORIGINAL_RESULT:/,
                ),
                idempotencyKey: expect.stringMatching(
                    /^w29:QUERY_ORIGINAL_RESULT:task-1:/,
                ),
            }),
        )
        expect(props.setLastResult).toHaveBeenCalledWith(resultValue)
    })

    it("requires linked evidence for evidence actions", async () => {
        const props = makeProps({
            item: makeItem({
                allowedActions: [...makeItem().allowedActions, "ADD_EVIDENCE"],
            }),
        })
        const { result } = renderActions(props)
        await act(async () => {
            await result.current.runTaskAction("ADD_EVIDENCE")
        })
        expect(props.setActionError).toHaveBeenCalledWith(
            "请先从受控证据入口关联已有证据",
        )
        expect(mocks.action.mutateAsync).not.toHaveBeenCalled()
    })

    it("reports action failures without rethrowing", async () => {
        mocks.action.mutateAsync.mockRejectedValue(new Error("服务不可用"))
        const props = makeProps()
        const { result } = renderActions(props)
        await act(async () => {
            await result.current.runTaskAction("QUERY_ORIGINAL_RESULT")
        })
        expect(props.setActionError).toHaveBeenCalledWith("服务不可用")
    })
})

describe("useIntegrationActions — close", () => {
    it("requires a replacement task for CLOSE_DUPLICATE", async () => {
        const props = makeProps()
        const { result } = renderActions(props)
        await expect(
            act(async () => {
                await result.current.handleClose("CLOSE_DUPLICATE")
            }),
        ).rejects.toThrow("请先选择替代任务")
        expect(props.setActionError).toHaveBeenCalledWith("请先选择替代任务")
    })

    it("closes a duplicate with the replacement task id", async () => {
        mocks.responsibility.mutateAsync.mockResolvedValue({})
        const props = makeProps()
        const { result } = renderActions(props)

        act(() => {
            result.current.setReplacementTaskId("rep-1")
        })
        await act(async () => {
            await result.current.handleClose("CLOSE_DUPLICATE")
        })

        expect(mocks.responsibility.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                kind: "CLOSE",
                reasonCode: "DUPLICATE",
                replacementWorkItemId: "rep-1",
            }),
        )
        expect(props.setLastResult).toHaveBeenCalledWith(
            expect.objectContaining({
                title: "已关闭重复任务",
                terminal: true,
                stayOnItem: false,
            }),
        )
    })

    it("closes a misrouted task without a replacement", async () => {
        mocks.responsibility.mutateAsync.mockResolvedValue({})
        const props = makeProps()
        const { result } = renderActions(props)
        await act(async () => {
            await result.current.handleClose("CLOSE_MISROUTED")
        })
        expect(mocks.responsibility.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                kind: "CLOSE",
                reasonCode: "MISROUTED",
                replacementWorkItemId: undefined,
            }),
        )
    })

    it("auto-advances to the neighbor after a terminal success", async () => {
        mocks.responsibility.mutateAsync.mockResolvedValue({})
        const next = makeItem({
            identity: {
                itemType: "ERROR_TASK",
                id: "task-2",
                number: "ET-2",
                subjectHash: "h2",
            },
        })
        const props = makeProps({ autoNext: true })
        vi.mocked(props.neighbor).mockReturnValue(next)
        const { result } = renderActions(props)

        await act(async () => {
            await result.current.handleClose("CLOSE_MISROUTED")
        })
        expect(props.goToItem).not.toHaveBeenCalled()
        act(() => {
            vi.advanceTimersByTime(400)
        })
        expect(props.goToItem).toHaveBeenCalledWith(next)
    })

    it("does not auto-advance in focus mode", async () => {
        mocks.responsibility.mutateAsync.mockResolvedValue({})
        const next = makeItem()
        const props = makeProps({ autoNext: true, focusMode: true })
        vi.mocked(props.neighbor).mockReturnValue(next)
        const { result } = renderActions(props)

        await act(async () => {
            await result.current.handleClose("CLOSE_MISROUTED")
        })
        act(() => {
            vi.advanceTimersByTime(400)
        })
        expect(props.goToItem).not.toHaveBeenCalled()
    })
})

describe("useIntegrationActions — resolve", () => {
    const resolvableItem = () =>
        makeItem({
            allowedActions: [...makeItem().allowedActions, "RESOLVE"],
            resolutionEvidencePolicy: {
                evidencePolicyId: "pol1",
                evidencePolicyVersion: 2,
                key: { errorType: "W29", fundsImpact: "NONE" },
                requiredEvidenceKinds: ["EXTERNAL_CASE_RESULT"],
                reviewerSeparation: "NONE",
            },
            linkedEvidence: [
                {
                    kind: "EXTERNAL_CASE_RESULT",
                    recordId: "r1",
                    label: "外部案例",
                },
            ],
        })

    it("blocks resolution until required evidence is linked", async () => {
        const item = resolvableItem()
        const props = makeProps({ item: { ...item, linkedEvidence: [] } })
        const { result } = renderActions(props)
        await act(async () => {
            await result.current.handleResolve()
        })
        expect(props.setActionError).toHaveBeenCalledWith(
            "完成凭证尚未齐备，请先从证据入口完成关联",
        )
        expect(mocks.resolve.mutateAsync).not.toHaveBeenCalled()
    })

    it("submits the terminal evidence resolution", async () => {
        mocks.resolve.mutateAsync.mockResolvedValue(
            makeResult({ terminal: true, stayOnItem: false }),
        )
        const props = makeProps({ item: resolvableItem() })
        const { result } = renderActions(props)

        await act(async () => {
            await result.current.handleResolve()
        })

        expect(mocks.resolve.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                itemType: "ERROR_TASK",
                itemId: "task-1",
                workItemId: "wi_1",
                reasonCode: "TERMINAL_EVIDENCE_VERIFIED",
                evidencePolicyId: "pol1",
                evidencePolicyVersion: 2,
                evidenceRefs: [
                    {
                        kind: "EXTERNAL_CASE_RESULT",
                        recordId: "r1",
                        label: "外部案例",
                    },
                ],
            }),
        )
    })
})

describe("useIntegrationActions — direct reconciliation", () => {
    it("requires a matching registered reason for a terminal conclusion", async () => {
        const props = makeProps({ item: differenceItem() })
        const { result } = renderActions(props)
        act(() => {
            result.current.setReconReasonId("COMPENSATION_CLOSED")
        })
        await act(async () => {
            await result.current.handleDirectTerminal("CONFIRM_NO_ERROR")
        })
        expect(props.setActionError).toHaveBeenCalledWith(
            "请选择与结论匹配的注册原因",
        )
        expect(mocks.direct.mutateAsync).not.toHaveBeenCalled()
    })

    it("submits the terminal conclusion for a matching reason", async () => {
        mocks.direct.mutateAsync.mockResolvedValue(
            makeResult({ terminal: true, stayOnItem: false }),
        )
        const props = makeProps({ item: differenceItem() })
        const { result } = renderActions(props)
        act(() => {
            result.current.setReconReasonId("BUSINESS_CONFIRMED_NO_ERROR")
        })
        await act(async () => {
            await result.current.handleDirectTerminal("CONFIRM_NO_ERROR")
        })
        expect(mocks.direct.mutateAsync).toHaveBeenCalledWith(
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
    })

    it("requires evidence for direct evidence actions", async () => {
        const props = makeProps({ item: differenceItem() })
        const { result } = renderActions(props)
        await act(async () => {
            await result.current.handleDirectAction("LINK_COMPENSATION")
        })
        expect(props.setActionError).toHaveBeenCalledWith(
            "请先从受控证据入口关联已有证据",
        )
        expect(mocks.direct.mutateAsync).not.toHaveBeenCalled()
    })

    it("submits a non-terminal direct action", async () => {
        mocks.direct.mutateAsync.mockResolvedValue(makeResult())
        const item = differenceItem({
            linkedEvidence: [
                {
                    kind: "COMPENSATION_RESULT",
                    recordId: "c1",
                    label: "补偿记录",
                },
            ],
        })
        const props = makeProps({ item })
        const { result } = renderActions(props)
        await act(async () => {
            await result.current.handleDirectAction("LINK_COMPENSATION")
        })
        expect(mocks.direct.mutateAsync).toHaveBeenCalledWith(
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
    })
})

describe("useIntegrationActions — state reset and derived values", () => {
    it("resets comment, replacement and reason when the item switches", () => {
        const props = makeProps({ item: differenceItem() })
        const { result, rerender } = renderActions(props)

        act(() => {
            result.current.setComment("备注")
            result.current.setReplacementTaskId("rep-9")
        })
        expect(result.current.reconReasonId).toBe("BUSINESS_CONFIRMED_NO_ERROR")
        expect(result.current.comment).toBe("备注")

        const other = differenceItem({
            identity: {
                itemType: "RECONCILIATION_DIFFERENCE",
                id: "diff-2",
                number: "RD-2",
                subjectHash: "h9",
            },
        })
        rerender({ ...props, item: other })

        expect(result.current.comment).toBe("")
        expect(result.current.replacementTaskId).toBe("")
        expect(result.current.reconReasonId).toBe("BUSINESS_CONFIRMED_NO_ERROR")
        expect(props.setActionError).toHaveBeenCalledWith(null)
    })

    it("exposes can() from the item allowed actions", () => {
        const { result } = renderActions(makeProps())
        expect(result.current.can("RESOLVE")).toBe(true)
        expect(result.current.can("ADD_EVIDENCE")).toBe(false)
    })

    it("exposes reasonMismatches against the selected reason", () => {
        const { result } = renderActions(makeProps({ item: differenceItem() }))
        expect(result.current.reasonMismatches("CONFIRM_NO_ERROR")).toBe(false)
        expect(
            result.current.reasonMismatches("CONFIRM_VALID_DIFFERENCE"),
        ).toBe(true)
    })

    it("aggregates pending mutations into formalPending", () => {
        mocks.action.isPending = true
        const { result } = renderActions(makeProps())
        expect(result.current.formalPending).toBe(true)
    })

    it("does not throw when focusing the action zone with no rendered DOM", () => {
        const { result } = renderActions(makeProps())
        act(() => {
            result.current.focusFirstAction()
        })
        act(() => {
            vi.advanceTimersByTime(250)
        })
        expect(result.current.actionZoneRef.current).toBeNull()
    })
})
