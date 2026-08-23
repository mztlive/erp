import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { act, cleanup, waitFor } from "@testing-library/react"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import {
    completeCardFundsReview,
    registerHistoricalInvoice,
    registerHistoricalReceipt,
} from "@/features/card-funds-review/api"
import type {
    FormalActionResponse,
    RegisterFundsResult,
} from "@/features/card-funds-review/types"
import { useCardFundsReviewWorkflow } from "./use-card-funds-review-workflow"
import { makeApprovedResponse, makeQueueView, makeTask } from "./test-data"

vi.mock("@/features/card-funds-review/api", () => ({
    fetchCardFundsReviewQueue: vi.fn(),
    completeCardFundsReview: vi.fn(),
    registerHistoricalReceipt: vi.fn(),
    registerHistoricalInvoice: vi.fn(),
}))

const mockedComplete = vi.mocked(completeCardFundsReview)
const mockedRegisterReceipt = vi.mocked(registerHistoricalReceipt)
const mockedRegisterInvoice = vi.mocked(registerHistoricalInvoice)
function renderWorkflow(
    overrides: {
        task?: ReturnType<typeof makeTask>
        tasks?: ReturnType<typeof makeTask>[]
        currentIndex?: number
        autoNext?: boolean
        context?: Partial<ReturnType<typeof makeQueueView>["context"]>
    } = {},
) {
    const task = overrides.task ?? makeTask()
    const tasks = overrides.tasks ?? [task]
    const view = makeQueueView(task)
    const context = { ...view.context, ...overrides.context }
    const replaceUrl = vi.fn()
    const setSearchInput = vi.fn()
    const client = createFreshQueryClient()
    const rendered = renderHookWithProviders(
        () =>
            useCardFundsReviewWorkflow({
                task,
                tasks,
                context,
                currentIndex: overrides.currentIndex ?? 0,
                queueContextId: "queue:card-funds-review:mine",
                autoNext: overrides.autoNext ?? false,
                replaceUrl,
                setSearchInput,
            }),
        { queryClient: client },
    )
    return { ...rendered, replaceUrl, setSearchInput, task }
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("useCardFundsReviewWorkflow · derived state", () => {
    it("derives assigned_to_me when decision actions are allowed", () => {
        const { result } = renderWorkflow({
            task: makeTask({
                workItem: { allowedActions: ["APPROVE", "REJECT"] },
            }),
        })
        expect(result.current.responsibilityStatus).toBe("assigned_to_me")
    })

    it("derives completed/closed from the work item status", () => {
        const completed = renderWorkflow({
            task: makeTask({
                workItem: { workItemStatus: "COMPLETED", allowedActions: [] },
            }),
        })
        expect(completed.result.current.responsibilityStatus).toBe("completed")

        const closed = renderWorkflow({
            task: makeTask({
                workItem: { workItemStatus: "CLOSED", allowedActions: [] },
            }),
        })
        expect(closed.result.current.responsibilityStatus).toBe("closed")
    })

    it("derives blocked / assigned_to_other from blockers and owner", () => {
        const blocked = renderWorkflow({
            task: makeTask({
                workItem: {
                    allowedActions: [],
                    actionBlockers: [
                        { action: "X", code: "B1", message: "无可用动作" },
                    ],
                },
            }),
        })
        expect(blocked.result.current.responsibilityStatus).toBe("blocked")

        const other = renderWorkflow({
            task: makeTask({
                workItem: {
                    allowedActions: [],
                    ownerUser: { id: "u_2", displayName: "别人" },
                },
            }),
        })
        expect(other.result.current.responsibilityStatus).toBe(
            "assigned_to_other",
        )
    })

    it("derives canConfirmZero only for zero-balance opening tasks", () => {
        const zero = renderWorkflow()
        expect(zero.result.current.canConfirmZero).toBe(true)

        const withBalance = renderWorkflow({
            task: makeTask({ account: { settledTotal: "10.00" } }),
        })
        expect(withBalance.result.current.canConfirmZero).toBe(false)

        const delta = renderWorkflow({
            task: makeTask({
                reviewType: "SYNC_DELTA",
                workItem: { workItemType: "CARD_FUNDS_DELTA_REVIEW" },
            }),
        })
        expect(delta.result.current.canConfirmZero).toBe(false)

        const noAction = renderWorkflow({
            task: makeTask({
                workItem: { allowedActions: ["APPROVE"] },
            }),
        })
        expect(noAction.result.current.canConfirmZero).toBe(false)
    })

    it("computes allocation totals from the draft lines", () => {
        const { result } = renderWorkflow()
        act(() => {
            result.current.openAllocation("receipt")
            result.current.setAllocLines([
                {
                    lineId: "a1",
                    targetAccountId: "acct_1",
                    targetLabel: "SO",
                    amount: "10.00",
                },
                {
                    lineId: "a2",
                    targetAccountId: "acct_1",
                    targetLabel: "SO",
                    amount: "abc",
                },
                {
                    lineId: "a3",
                    targetAccountId: "acct_1",
                    targetLabel: "SO",
                    amount: "5.50",
                },
            ])
            result.current.setReceiptForm((f) => ({
                ...f,
                grossAmount: "20.00",
            }))
        })
        expect(result.current.allocatedSum).toBe(15.5)
        expect(result.current.allocTarget).toBe(20)
    })
})

describe("useCardFundsReviewWorkflow · navigation", () => {
    it("goToWorkItem resets results and writes the URL patch", () => {
        const { result, replaceUrl } = renderWorkflow()
        act(() => {
            result.current.goToWorkItem("wi_5")
        })
        expect(replaceUrl).toHaveBeenCalledWith({
            currentWorkItemId: "wi_5",
            queueContextId: "queue:card-funds-review:mine",
        })

        act(() => {
            result.current.goToWorkItem(null)
        })
        expect(replaceUrl).toHaveBeenLastCalledWith({
            currentWorkItemId: null,
            queueContextId: "queue:card-funds-review:mine",
        })
    })

    it("clearFilters resets the search input and clears queue filters", () => {
        const { result, replaceUrl, setSearchInput } = renderWorkflow()
        act(() => {
            result.current.clearFilters()
        })
        expect(setSearchInput).toHaveBeenCalledWith("")
        expect(replaceUrl).toHaveBeenCalledWith({
            type: null,
            status: null,
            due: null,
            q: null,
            currentWorkItemId: null,
        })
    })

    it("neighborId walks the task list around the current index", () => {
        const first = makeTask({ workItem: { workItemId: "wi_1" } })
        const second = makeTask({ workItem: { workItemId: "wi_2" } })
        const third = makeTask({ workItem: { workItemId: "wi_3" } })
        const { result } = renderWorkflow({
            task: second,
            tasks: [first, second, third],
            currentIndex: 1,
        })
        expect(result.current.neighborId(1)).toBe("wi_3")
        expect(result.current.neighborId(-1)).toBe("wi_1")
        expect(result.current.neighborId(2)).toBeUndefined()
        expect(result.current.neighborId(-2)).toBeUndefined()
    })
})

describe("useCardFundsReviewWorkflow · decision submission", () => {
    function renderWithEvidence() {
        const rendered = renderWorkflow()
        act(() => {
            rendered.result.current.setEvidenceRef("银行回单 001")
        })
        return rendered
    }

    it("approves and shows the succeeded result", async () => {
        mockedComplete.mockResolvedValue(makeApprovedResponse())
        const { result } = renderWithEvidence()

        await act(async () => {
            await result.current.runApprove("RECORDED_FACTS_RECONCILED", false)
        })
        expect(mockedComplete).toHaveBeenCalledTimes(1)
        const command = mockedComplete.mock.calls[0]![0]
        expect(command.workItemId).toBe("wi_1")
        expect(command.expectedTaskVersion).toBe("tv_1")
        expect(command.idempotencyKey).toBe(
            "w13:wi_1:tv_1:approve:RECORDED_FACTS_RECONCILED",
        )
        expect(command.decision.reviewResult).toBe("APPROVED")
        expect(command.decision.evidenceReferences).toEqual(["银行回单 001"])
        expect(result.current.confirmMode).toBeNull()
        expect(result.current.lastResult?.status).toBe("succeeded")
        expect(result.current.lastResult?.title).toBe("复核通过 · 复核号 7")
        expect(result.current.lastResult?.stayOnItem).toBe(true)
    })

    it("confirms from zero with CONFIRM_ZERO and a zero conclusion", async () => {
        mockedComplete.mockResolvedValue(makeApprovedResponse())
        const { result } = renderWithEvidence()

        await act(async () => {
            await result.current.runApprove("NO_HISTORY_FROM_ZERO", true)
        })
        const command = mockedComplete.mock.calls[0]![0]
        expect(command.decision.conclusion).toBe("NO_HISTORY_FROM_ZERO")
    })

    it("surfaces the server failure message as the action error", async () => {
        mockedComplete.mockResolvedValue({
            status: "failed",
            code: "SUBJECT_HASH_MISMATCH",
            message: "数据版本不匹配",
        } satisfies FormalActionResponse)
        const { result } = renderWithEvidence()

        await act(async () => {
            await result.current.runApprove("RECORDED_FACTS_RECONCILED", false)
        })
        expect(result.current.actionError).toBe("数据版本不匹配")
        expect(result.current.lastResult).toBeNull()
    })

    it("records the unknown result for unclear outcomes", async () => {
        mockedComplete.mockResolvedValue({
            status: "unknown",
            idempotencyKey: "k1",
            message: "请求结果尚未确认",
        } satisfies FormalActionResponse)
        const { result } = renderWithEvidence()

        await act(async () => {
            await result.current.runApprove("RECORDED_FACTS_RECONCILED", false)
        })
        expect(result.current.lastResult?.status).toBe("unknown")
        expect(result.current.lastResult?.title).toBe("复核结果待确认")
        expect(result.current.formalPending).toBe(true)
    })

    it("rejects and shows the collaboration message of the follow-up", async () => {
        mockedComplete.mockResolvedValue({
            status: "succeeded",
            outcome: {
                kind: "REJECTED",
                business: {
                    receivableFundsReviewId: "rfr_2",
                    receivableAccountId: "acct_1",
                    reviewNo: 8,
                    accountReviewStatus: "rejected",
                    workflowActionId: "wa_2",
                    operationId: "op_2",
                    completedAt: "2026-07-01T09:00:00.000Z",
                    reviewResult: "REJECTED",
                    conclusion: "REJECTED",
                    followUpConfiguration: {
                        status: "BLOCKED",
                        blockerCode:
                            "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED",
                        collaborationMessage: "驳回后继未配置",
                        requiredRegistration: [],
                    },
                },
            },
        } satisfies FormalActionResponse)
        const { result } = renderWithEvidence()

        await act(async () => {
            await result.current.submitReject({
                reasonCode: "FACTS_MISMATCH",
                comment: "票款记录不一致的说明",
            })
        })
        expect(result.current.lastResult?.status).toBe("rejected")
        expect(result.current.lastResult?.title).toBe("已驳回 · 复核号 8")
        expect(result.current.lastResult?.description).toBe("驳回后继未配置")
        const command = mockedComplete.mock.calls[0]![0]
        const decision = command.decision
        expect(decision.reviewResult).toBe("REJECTED")
        if (decision.reviewResult !== "REJECTED") {
            throw new Error("expected a REJECTED decision")
        }
        expect(decision.reasonCode).toBe("FACTS_MISMATCH")
    })

    it("blocks submission when evidence is missing", async () => {
        const { result } = renderWorkflow()
        await act(async () => {
            await result.current.runApprove("RECORDED_FACTS_RECONCILED", false)
        })
        expect(mockedComplete).not.toHaveBeenCalled()
        expect(result.current.actionError).toBe(
            "正式复核必须提供凭证编号或证据说明",
        )
    })

    it("advances to the next item after a short stay when autoNext is on", async () => {
        mockedComplete.mockResolvedValue(makeApprovedResponse())
        const first = makeTask({ workItem: { workItemId: "wi_1" } })
        const second = makeTask({ workItem: { workItemId: "wi_2" } })
        const { result, replaceUrl } = renderWorkflow({
            task: first,
            tasks: [first, second],
            currentIndex: 0,
            autoNext: true,
            context: { nextWorkItemId: "wi_2" },
        })
        act(() => {
            result.current.setEvidenceRef("银行回单 001")
        })
        await act(async () => {
            await result.current.runApprove("RECORDED_FACTS_RECONCILED", true)
        })
        expect(result.current.lastResult?.stayOnItem).toBe(false)
        expect(replaceUrl).not.toHaveBeenCalled()
        await waitFor(
            () =>
                expect(replaceUrl).toHaveBeenCalledWith({
                    currentWorkItemId: "wi_2",
                    queueContextId: "queue:card-funds-review:mine",
                }),
            { timeout: 4000 },
        )
    })
})

describe("useCardFundsReviewWorkflow · registration", () => {
    const receiptResult: RegisterFundsResult = {
        fundsFactVersion: "ffv_2",
        subjectHash: "sh_2",
        settledTotal: "99.00",
        invoicedTotal: "0.00",
        openTotal: "1031.00",
        openInvoiceableTotal: "1130.00",
        receiptFacts: [],
        invoiceFacts: [],
    }

    it("registers a historical receipt with the draft and allocation lines", async () => {
        mockedRegisterReceipt.mockResolvedValue(receiptResult)
        const { result } = renderWorkflow()
        act(() => {
            result.current.setReceiptForm({
                receiptNo: "SK-001",
                receivedAt: "2026-07-02",
                grossAmount: "99.00",
            })
        })
        act(() => {
            result.current.openAllocation("receipt")
        })
        await act(async () => {
            await result.current.submitReceipt()
        })
        expect(mockedRegisterReceipt.mock.calls[0]?.[0]).toEqual(
            expect.objectContaining({
                workItemId: "wi_1",
                receiptNo: "SK-001",
                receivedAt: "2026-07-02",
                grossAmount: "99.00",
                allocations: expect.arrayContaining([
                    expect.objectContaining({
                        targetAccountId: "acct_1",
                        amount: "99.00",
                    }),
                ]),
                evidenceReference: "银行回单-本次登记",
            }),
        )
        expect(result.current.allocationMode).toBeNull()
        expect(result.current.lastResult?.status).toBe("succeeded")
        expect(result.current.lastResult?.title).toBe("历史回款已登记")
    })

    it("generates a receipt number when the draft is blank", async () => {
        mockedRegisterReceipt.mockResolvedValue(receiptResult)
        const { result } = renderWorkflow()
        act(() => {
            result.current.setReceiptForm((f) => ({
                ...f,
                grossAmount: "99.00",
            }))
        })
        await act(async () => {
            await result.current.submitReceipt()
        })
        expect(mockedRegisterReceipt.mock.calls[0]?.[0]).toEqual(
            expect.objectContaining({
                receiptNo: expect.stringMatching(/^SK-[A-Z0-9]+$/),
            }),
        )
    })

    it("surfaces registration errors without leaving the editor", async () => {
        mockedRegisterReceipt.mockRejectedValue(new Error("禁止创建 0 元回款"))
        const { result } = renderWorkflow()
        act(() => {
            result.current.setReceiptForm((f) => ({ ...f, grossAmount: "0" }))
        })
        await act(async () => {
            await result.current.submitReceipt()
        })
        expect(result.current.actionError).toBe("禁止创建 0 元回款")
        expect(result.current.lastResult).toBeNull()
    })

    it("derives net and tax from the gross amount when registering an invoice", async () => {
        mockedRegisterInvoice.mockResolvedValue({
            ...receiptResult,
            invoicedTotal: "113.00",
            receiptFacts: [],
        })
        const { result } = renderWorkflow()
        act(() => {
            result.current.setInvoiceForm({
                invoiceNo: "FP-001",
                issuedAt: "2026-07-03",
                grossAmount: "113.00",
                netAmount: "",
                taxAmount: "",
            })
        })
        await act(async () => {
            await result.current.submitInvoice()
        })
        expect(mockedRegisterInvoice.mock.calls[0]?.[0]).toEqual(
            expect.objectContaining({
                invoiceNo: "FP-001",
                grossAmount: "113.00",
                netAmount: "100.00",
                taxAmount: "13.00",
                evidenceReference: "发票扫描件-本次登记",
            }),
        )
        expect(result.current.lastResult?.status).toBe("succeeded")
        expect(result.current.lastResult?.title).toBe("历史发票已登记")
    })
})

describe("useCardFundsReviewWorkflow · keyboard shortcut", () => {
    function pressKey(init: KeyboardEventInit) {
        window.dispatchEvent(new KeyboardEvent("keydown", init))
    }

    it("hints when evidence is missing on meta+Enter", () => {
        const { result } = renderWorkflow()
        act(() => {
            pressKey({ key: "Enter", metaKey: true })
        })
        expect(result.current.keyHint).toBe(
            "请先填写凭证编号或证据说明；证据将随正式决定一并提交。",
        )
        expect(result.current.confirmMode).toBeNull()
    })

    it("opens the approve confirm with evidence present", () => {
        const { result } = renderWorkflow({
            task: makeTask({ account: { settledTotal: "10.00" } }),
        })
        act(() => {
            result.current.setEvidenceRef("银行回单 001")
        })
        act(() => {
            pressKey({ key: "Enter", metaKey: true })
        })
        expect(result.current.confirmMode).toEqual({
            kind: "approve",
            conclusion: "RECORDED_FACTS_RECONCILED",
            advance: false,
        })
    })

    it("opens the zero confirm for zero-balance opening tasks", () => {
        const { result } = renderWorkflow()
        act(() => {
            result.current.setEvidenceRef("银行回单 001")
        })
        act(() => {
            pressKey({ key: "Enter", ctrlKey: true })
        })
        expect(result.current.confirmMode).toEqual({
            kind: "zero",
            advance: false,
        })
    })

    it("hints to refresh when the task disallows submission", () => {
        const { result } = renderWorkflow({
            task: makeTask({ workItem: { allowedActions: [] } }),
        })
        act(() => {
            pressKey({ key: "Enter", metaKey: true })
        })
        expect(result.current.keyHint).toBe(
            "当前责任或任务状态不允许提交，请刷新后重试。",
        )
    })
})
