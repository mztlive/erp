import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act } from '@testing-library/react'

const formMocks = vi.hoisted(() => {
    type FakeFormOpts = {
        defaultValues?: Record<string, unknown>
        onSubmit?: (arg: { value: Record<string, unknown> }) => Promise<void>
    }
    const makeFakeForm = (opts: FakeFormOpts = {}) => {
        const values: Record<string, unknown> = { ...opts.defaultValues }
        return {
            AppField: () => null,
            AppForm: () => null,
            SubmitButton: () => null,
            getFieldValue: (name: string) => values[name],
            setFieldValue: (name: string, value: unknown) => {
                values[name] = value
            },
            handleSubmit: async () => {
                await opts.onSubmit?.({ value: { ...values } })
            },
        }
    }
    return {
        useAppForm: vi.fn((opts?: FakeFormOpts) => makeFakeForm(opts)),
    }
})

vi.mock('@/components/form', () => ({
    useAppForm: formMocks.useAppForm,
}))

const queryMocks = vi.hoisted(() => ({
    submitDecision: vi.fn(),
    cancelApproval: vi.fn(),
}))

vi.mock('@/features/sales-orders/hooks/queries', () => ({
    salesOrderKeys: {
        detail: (id: string) => ['sales-orders', 'detail', id],
    },
    useSubmitCardSalesApprovalDecisionMutation: vi.fn(() => ({
        mutateAsync: queryMocks.submitDecision,
        isPending: false,
    })),
    useCancelCardSalesApprovalMutation: vi.fn(() => ({
        mutateAsync: queryMocks.cancelApproval,
        isPending: false,
    })),
}))

const workItemMocks = vi.hoisted(() => ({
    responsibility: vi.fn(),
}))

vi.mock('@/features/work-items', () => ({
    useWorkItemResponsibilityMutation: vi.fn(() => ({
        mutateAsync: workItemMocks.responsibility,
        isPending: false,
    })),
}))

vi.mock('@/lib/api/errors', () => ({
    getErrorPresentation: vi.fn((_error: unknown, fallback: string) => ({
        title: fallback,
        description: fallback,
    })),
}))

import { getErrorPresentation } from '@/lib/api/errors'
import type { CardSalesApproval, SalesOrderListItem } from '@/features/sales-orders/types'
import {
    actionKey,
    approvalDecision,
    cancelActionKey,
    isUncertainResult,
    rejectSchema,
    useCardSalesApprovalActions,
} from '@/features/sales-orders/hooks/use-card-approval-actions'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'

const makeOrder = (): SalesOrderListItem =>
    ({
        id: 'so-1',
        documentNumber: 'SO-2026-001',
        lockVersion: 7,
    }) as SalesOrderListItem

type ReadyApproval = Extract<
    CardSalesApproval,
    { processingState: 'READY' }
>

const makeReadyApproval = (
    overrides: Partial<ReadyApproval> = {},
): ReadyApproval =>
    ({
        approvalInstanceId: 'ai-1',
        instanceVersion: '1',
        approvalStepInstanceId: 'as-1',
        stepVersion: '2',
        processingState: 'READY',
        subjectVersion: 'sv-1',
        salesOrderSubmissionId: 'sub-1',
        submissionNo: 3,
        frozenSubmissionSummary: '冻结摘要',
        expectedReviewStatus: 'PENDING_SALES_LEAD',
        allowedActions: [
            'START_PROCESSING',
            'APPROVE',
            'REJECT',
            'TERMINATE',
            'CANCEL',
        ],
        actionBlockers: [],
        workItemId: 'wi-1',
        workItemType: 'CARD_SALES_MANAGER_APPROVAL',
        taskVersion: '4',
        workItemStatus: 'OPEN',
        assignmentMode: 'POOL',
        ...overrides,
    }) as ReadyApproval

const makeBlockedApproval = (): CardSalesApproval =>
    ({
        ...makeReadyApproval(),
        processingState: 'APPROVAL_BLOCKED',
        workItemStatus: undefined,
        workItemType: undefined,
        workItemId: undefined,
        taskVersion: undefined,
        assignmentMode: undefined,
        allowedActions: ['CANCEL'],
    }) as unknown as CardSalesApproval

describe('card approval pure helpers', () => {
    it('builds action keys from task identity', () => {
        const approval = makeReadyApproval()
        expect(actionKey(approval, 'APPROVE')).toBe('w05:wi-1:4:APPROVE')
        expect(actionKey(approval, 'START_PROCESSING')).toBe(
            'w05:wi-1:4:START_PROCESSING',
        )
    })

    it('builds cancel keys from approval instance identity', () => {
        expect(cancelActionKey(makeReadyApproval())).toBe(
            'w05:ai-1:1:as-1:2:CANCEL',
        )
    })

    it('maps manager approvals to the sales lead review step', () => {
        const approval = makeReadyApproval()
        const order = makeOrder()

        expect(
            approvalDecision(order, approval, 'APPROVE'),
        ).toMatchObject({
            workItemType: 'CARD_SALES_MANAGER_APPROVAL',
            expectedReviewStatus: 'PENDING_SALES_LEAD',
            reviewDecision: 'APPROVE',
            expectedSalesOrderLockVersion: 7,
            expectedSubmissionNo: 3,
        })
        expect(
            approvalDecision(order, approval, 'REJECT', {
                reasonCode: '资料不齐',
                comment: '补充材料',
            }),
        ).toMatchObject({
            reviewDecision: 'REJECT',
            reasonCode: '资料不齐',
            comment: '补充材料',
        })
    })

    it('maps operation approvals to the operations review step', () => {
        const approval = makeReadyApproval({
            workItemType: 'CARD_SALES_OPERATION_APPROVAL',
            expectedReviewStatus: 'PENDING_OPERATIONS',
        })
        const order = makeOrder()

        expect(
            approvalDecision(order, approval, 'APPROVE'),
        ).toMatchObject({
            workItemType: 'CARD_SALES_OPERATION_APPROVAL',
            expectedReviewStatus: 'PENDING_OPERATIONS',
            reviewDecision: 'APPROVE',
        })
        expect(
            approvalDecision(order, approval, 'TERMINATE', {
                reasonCode: '业务取消',
                comment: '不再采购',
            }),
        ).toMatchObject({
            reviewDecision: 'TERMINATE',
            reasonCode: '业务取消',
        })
    })

    it('recognizes uncertain network/parse failures', () => {
        expect(isUncertainResult({ kind: 'Network' })).toBe(true)
        expect(isUncertainResult({ kind: 'Parse' })).toBe(true)
        expect(isUncertainResult({ kind: 'Business' })).toBe(false)
        expect(isUncertainResult(new Error('x'))).toBe(false)
        expect(isUncertainResult(null)).toBe(false)
    })

    it('validates reject reason payloads', () => {
        expect(
            rejectSchema.safeParse({ reasonCode: '分类', comment: '说明文字' })
                .success,
        ).toBe(true)
        expect(
            rejectSchema.safeParse({ reasonCode: '短', comment: '说明文字' })
                .success,
        ).toBe(false)
        expect(
            rejectSchema.safeParse({ reasonCode: '分类', comment: '短' })
                .success,
        ).toBe(false)
    })
})

describe('useCardSalesApprovalActions', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        queryMocks.submitDecision.mockReset()
        queryMocks.cancelApproval.mockReset()
        workItemMocks.responsibility.mockReset()
    })

    const renderActions = (
        approval: CardSalesApproval = makeReadyApproval(),
    ) => {
        const onResult = vi.fn()
        const { result } = renderHookWithProviders(() =>
            useCardSalesApprovalActions({
                order: makeOrder(),
                approval,
                onResult,
            }),
        )
        return { result, onResult }
    }

    it('derives action eligibility from the server projection', () => {
        const { result } = renderActions()
        expect(result.current.canStart).toBe(true)
        expect(result.current.canApprove).toBe(true)
        expect(result.current.canReject).toBe(true)
        expect(result.current.canTerminate).toBe(true)
        expect(result.current.canCancel).toBe(true)
        expect(result.current.isManager).toBe(true)
    })

    it('withholds decisions for direct assignments without start eligibility', () => {
        const { result } = renderActions(
            makeReadyApproval({
                assignmentMode: 'DIRECT',
                allowedActions: ['APPROVE', 'REJECT'],
            }),
        )
        expect(result.current.canStart).toBe(false)
        expect(result.current.canApprove).toBe(true)
        expect(result.current.canReject).toBe(true)
        expect(result.current.canTerminate).toBe(false)
        expect(result.current.canCancel).toBe(false)
    })

    it('keeps cancel available for blocked approvals without tasks', () => {
        const { result } = renderActions(makeBlockedApproval())
        expect(result.current.canCancel).toBe(true)
        expect(result.current.canStart).toBe(false)
        expect(result.current.canApprove).toBe(false)
    })

    it('starts processing with a frozen idempotency key and refreshes detail', async () => {
        workItemMocks.responsibility.mockResolvedValue({})
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const onResult = vi.fn()
        const { result } = renderHookWithProviders(
            () =>
                useCardSalesApprovalActions({
                    order: makeOrder(),
                    approval: makeReadyApproval(),
                    onResult,
                }),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.startProcessing()
        })

        expect(workItemMocks.responsibility).toHaveBeenCalledWith({
            kind: 'START_PROCESSING',
            workItemId: 'wi-1',
            expectedTaskVersion: '4',
            idempotencyKey: 'w05:wi-1:4:START_PROCESSING',
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['sales-orders', 'detail', 'so-1'],
        })
        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({ status: 'succeeded' }),
        )
    })

    it('reports uncertain start results without failing', async () => {
        workItemMocks.responsibility.mockRejectedValue({ kind: 'Network' })
        const onResult = vi.fn()
        const { result } = renderHookWithProviders(
            () =>
                useCardSalesApprovalActions({
                    order: makeOrder(),
                    approval: makeReadyApproval(),
                    onResult,
                }),
        )

        await act(async () => {
            await result.current.startProcessing()
        })

        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: 'unknown',
                reference: 'w05:wi-1:4:START_PROCESSING',
            }),
        )
    })

    it('turns the reject form payload into a confirm dialog', async () => {
        const { result } = renderActions()
        const rejectOpts = formMocks.useAppForm.mock.calls[0]![0]!

        await act(async () => {
            await rejectOpts.onSubmit!({
                value: { reasonCode: ' 资料不齐 ', comment: ' 请补充资质 ' },
            })
        })

        expect(result.current.confirmReject).toBe(true)
        expect(result.current.confirmApprove).toBe(false)
    })

    it('turns the terminate form payload into a confirm dialog', async () => {
        const { result } = renderActions()
        const terminateOpts = formMocks.useAppForm.mock.calls[1]![0]!

        await act(async () => {
            await terminateOpts.onSubmit!({
                value: { reasonCode: '业务取消', comment: '不再采购了' },
            })
        })

        expect(result.current.confirmTerminate).toBe(true)
    })

    it('submits an approve decision and reports the manager outcome', async () => {
        queryMocks.submitDecision.mockResolvedValue({
            business_result: {
                outcome: 'MANAGER_APPROVED',
                workflow_action_id: 'wf-1',
            },
            approval: { instance: { status: 'APPROVED' } },
        })
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.confirmApproveDecision()
        })

        expect(queryMocks.submitDecision).toHaveBeenCalledWith(
            expect.objectContaining({
                approvalInstanceId: 'ai-1',
                expectedInstanceVersion: '1',
                approvalStepInstanceId: 'as-1',
                expectedStepVersion: '2',
                workItemId: 'wi-1',
                expectedTaskVersion: '4',
                idempotencyKey: 'w05:wi-1:4:APPROVE',
            }),
        )
        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: 'succeeded',
                nextResponsible: '运营',
            }),
        )
    })

    it('reports a blocked approve when the step stays blocked', async () => {
        queryMocks.submitDecision.mockResolvedValue({
            business_result: {
                outcome: 'MANAGER_APPROVED',
                workflow_action_id: 'wf-1',
            },
            approval: { instance: { status: 'BLOCKED' } },
        })
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.confirmApproveDecision()
        })

        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: 'blocked',
                nextResponsible: '审批管理员',
            }),
        )
    })

    it('reports uncertain approve outcomes with the retryable key', async () => {
        queryMocks.submitDecision.mockRejectedValue({ kind: 'Parse' })
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.confirmApproveDecision()
        })

        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: 'unknown',
                reference: 'w05:wi-1:4:APPROVE',
            }),
        )
    })

    it('presents business rejections for approve decisions', async () => {
        queryMocks.submitDecision.mockRejectedValue(new Error('版本已过期'))
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.confirmApproveDecision()
        })

        expect(getErrorPresentation).toHaveBeenCalled()
        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({ status: 'blocked' }),
        )
    })

    it('submits a reject decision carrying the recorded reason', async () => {
        queryMocks.submitDecision.mockResolvedValue({
            business_result: { outcome: 'REJECTED_TO_SALES', workflow_action_id: 'wf-2' },
            approval: { instance: { status: 'REJECTED' } },
        })
        const { result, onResult } = renderActions()
        const rejectOpts = formMocks.useAppForm.mock.calls[0]![0]!

        await act(async () => {
            await rejectOpts.onSubmit!({
                value: { reasonCode: '资料不齐', comment: '请补充资质' },
            })
        })
        await act(async () => {
            await result.current.confirmRejectDecision()
        })

        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: 'rejected',
                reference: 'wf-2',
                nextResponsible: '销售',
            }),
        )
        const decision = queryMocks.submitDecision.mock.calls[0][0]
        expect(decision.decision).toMatchObject({
            reviewDecision: 'REJECT',
            reasonCode: '资料不齐',
            comment: '请补充资质',
        })
        expect(decision.idempotencyKey).toBe('w05:wi-1:4:REJECT')
    })

    it('skips the reject submission when no payload was recorded', async () => {
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.confirmRejectDecision()
        })

        expect(queryMocks.submitDecision).not.toHaveBeenCalled()
        expect(onResult).not.toHaveBeenCalled()
    })

    it('submits a terminate decision and reports success', async () => {
        queryMocks.submitDecision.mockResolvedValue({
            business_result: { outcome: 'TERMINATED', workflow_action_id: 'wf-3' },
            approval: { instance: { status: 'TERMINATED' } },
        })
        const { result, onResult } = renderActions()
        const terminateOpts = formMocks.useAppForm.mock.calls[1]![0]!

        await act(async () => {
            await terminateOpts.onSubmit!({
                value: { reasonCode: '业务取消', comment: '不再采购了' },
            })
        })
        await act(async () => {
            await result.current.confirmTerminateDecision()
        })

        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: 'succeeded',
                reference: 'wf-3',
                nextResponsible: '销售',
            }),
        )
        expect(queryMocks.submitDecision).toHaveBeenCalledWith(
            expect.objectContaining({ idempotencyKey: 'w05:wi-1:4:TERMINATE' }),
        )
    })

    it('cancels the approval and reports the draft outcome', async () => {
        queryMocks.cancelApproval.mockResolvedValue({
            business_result: { sales_order_commercial_status: 'DRAFT' },
        })
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.confirmCancelDecision()
        })

        expect(queryMocks.cancelApproval).toHaveBeenCalledWith(
            expect.objectContaining({
                approvalInstanceId: 'ai-1',
                currentStepInstanceId: 'as-1',
                workItemId: 'wi-1',
                expectedInstanceVersion: '1',
                expectedStepVersion: '2',
                expectedTaskVersion: '4',
                expectedSubjectVersion: 'sv-1',
                reason: '申请人撤回并继续修改',
                idempotencyKey: 'w05:ai-1:1:as-1:2:CANCEL',
            }),
        )
        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: 'succeeded',
                description:
                    '销售单已恢复为草稿，可以修改后重新提交。',
                nextResponsible: '销售',
            }),
        )
    })

    it('reports uncertain cancel outcomes without failing', async () => {
        queryMocks.cancelApproval.mockRejectedValue({ kind: 'Network' })
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.confirmCancelDecision()
        })

        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({ status: 'unknown' }),
        )
    })
})
