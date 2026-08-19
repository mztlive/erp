import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act } from '@testing-library/react'

import { renderHookWithProviders } from '@/features/test-utils'
import { useReverseFlow } from './use-reverse-flow'
import type {
    CustomerRefundRow,
    ReceiptReversalRow,
} from '@/features/customer-receivables/types'

const {
    mutateAsyncMock,
    ensureRefundMock,
    submitRefundMock,
    ensureReversalMock,
    submitReversalMock,
} = vi.hoisted(() => ({
    mutateAsyncMock: vi.fn(),
    ensureRefundMock: vi.fn(),
    submitRefundMock: vi.fn(),
    ensureReversalMock: vi.fn(),
    submitReversalMock: vi.fn(),
}))

vi.mock('@/features/customer-receivables/hooks/queries', () => ({
    useReverseFactMutation: () => ({
        mutateAsync: mutateAsyncMock,
        isPending: false,
    }),
    useEnsureCustomerRefundDraftMutation: () => ({
        mutateAsync: ensureRefundMock,
        isPending: false,
    }),
    useSubmitCustomerRefundMutation: () => ({
        mutateAsync: submitRefundMock,
        isPending: false,
    }),
    useEnsureReceiptReversalDraftMutation: () => ({
        mutateAsync: ensureReversalMock,
        isPending: false,
    }),
    useSubmitReceiptReversalMutation: () => ({
        mutateAsync: submitReversalMock,
        isPending: false,
    }),
}))

const refundRow = (overrides: Partial<CustomerRefundRow> = {}): CustomerRefundRow => ({
    refundId: 'crf-1',
    refundNo: 'TK-1',
    customerId: 'c1',
    originalReceiptId: 'src_3',
    reasonText: '退差额',
    amount: '50.00',
    occurredAt: '',
    status: 'draft',
    statusLabel: '草稿',
    statusTone: 'neutral',
    baselineVersion: 1,
    allowedActions: ['VIEW_DETAIL'],
    actionBlockers: [],
    approval: {
        requirement: 'PROCESS_REQUIRED',
        definition: {
            id: 'def-crf-1',
            name: '客户退款审批',
            version: 1,
            nodes: [{ key: 'n1', name: '退款复核', assigneeName: '张三' }],
            publishedNodes: [],
        },
        recentHistory: [],
        historyHasMore: false,
        allowedActions: ['SUBMIT'],
    },
    ...overrides,
})

const reversalRow = (
    overrides: Partial<ReceiptReversalRow> = {},
): ReceiptReversalRow => ({
    reversalId: 'rr-1',
    reversalNo: 'CZ-1',
    originalReceiptId: 'src_2',
    reasonText: '录入错误',
    amount: '80.00',
    occurredAt: '',
    status: 'draft',
    statusLabel: '草稿',
    statusTone: 'neutral',
    baselineVersion: 1,
    allowedActions: ['VIEW_DETAIL'],
    actionBlockers: [],
    approval: {
        requirement: 'PROCESS_REQUIRED',
        definition: {
            id: 'def-rr-1',
            name: '回款冲正审批',
            version: 1,
            nodes: [{ key: 'n1', name: '冲正复核', assigneeName: '张三' }],
            publishedNodes: [],
        },
        recentHistory: [],
        historyHasMore: false,
        allowedActions: ['SUBMIT'],
    },
    ...overrides,
})

function setup() {
    const closePreview = vi.fn()
    const openRefundPreview = vi.fn()
    const openReversalPreview = vi.fn()
    const setLastResult = vi.fn()
    const setActionError = vi.fn()
    const args = {
        closePreview,
        openRefundPreview,
        openReversalPreview,
        setLastResult,
        setActionError,
    }
    const rendered = renderHookWithProviders(() => useReverseFlow(args))
    return {
        ...rendered,
        args,
        closePreview,
        openRefundPreview,
        openReversalPreview,
        setLastResult,
        setActionError,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe('useReverseFlow', () => {
    it('submits a red invoice reverse with amount and default reason', async () => {
        mutateAsyncMock.mockResolvedValue({
            status: 'succeeded',
            reverseFactId: 'rf_1',
            reverseFactNo: 'RF-1',
            operationId: 'op_1',
            message: '已记录',
        })
        const { result } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'red_invoice',
                sourceFactId: 'src_1',
                label: 'INV-1',
                amount: '100.00',
            })
            result.current.setReverseAmount('80.00')
        })
        await act(async () => {
            await result.current.confirmReverse()
        })
        expect(mutateAsyncMock).toHaveBeenCalledWith({
            kind: 'red_invoice',
            sourceFactId: 'src_1',
            amount: '80.00',
            reason: '纠错',
            idempotencyKey: expect.stringMatching(/^w11-rev-src_1-\d+$/),
        })
    })

    it('creates a receipt reversal draft then submits approval without posting', async () => {
        ensureReversalMock.mockResolvedValue({
            status: 'succeeded',
            reversal: reversalRow(),
        })
        submitReversalMock.mockResolvedValue({
            status: 'succeeded',
            reversal: reversalRow({
                status: 'in_approval',
                statusLabel: '审批中',
                baselineVersion: 2,
            }),
        })
        const { result, openReversalPreview, setLastResult, setActionError } =
            setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'receipt_reverse',
                sourceFactId: 'src_2',
                label: 'RCP-2',
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft('录入错误')
        })
        expect(ensureReversalMock).toHaveBeenCalledWith(
            expect.objectContaining({
                sourceFactId: 'src_2',
                reason: '录入错误',
            }),
        )
        expect(mutateAsyncMock).not.toHaveBeenCalled()
        expect(openReversalPreview).toHaveBeenCalledWith('rr-1')
        expect(result.current.reversalSubmitOpen).toBe(true)
        expect(result.current.reverseConfirm).toBeNull()

        await act(async () => {
            await result.current.confirmReversalSubmit()
        })
        expect(submitReversalMock).toHaveBeenCalledWith({
            reversalId: 'rr-1',
            expectedVersion: 1,
            idempotencyKey: expect.stringMatching(/^w11-rr-src_2-/),
        })
        expect(setLastResult).toHaveBeenCalledWith({
            status: 'succeeded',
            title: '冲正已提交审批',
            description: '已按已绑定的审批流程启动审批，原回款保留。',
            reference: expect.stringMatching(/^w11-rr-src_2-/),
            facts: [
                { label: '冲正单号', value: 'CZ-1' },
                { label: '当前状态', value: '审批中' },
            ],
        })
        expect(setActionError).not.toHaveBeenCalled()
        expect(result.current.reversalSubmitOpen).toBe(false)
    })

    it('creates a refund draft then submits approval without posting', async () => {
        ensureRefundMock.mockResolvedValue({
            status: 'succeeded',
            refund: refundRow(),
        })
        submitRefundMock.mockResolvedValue({
            status: 'succeeded',
            refund: refundRow({
                status: 'in_approval',
                statusLabel: '审批中',
                baselineVersion: 2,
            }),
        })
        const { result, openRefundPreview, setLastResult, setActionError } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'refund',
                sourceFactId: 'src_3',
                label: 'RCP-3',
                amount: '50.00',
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft('退差额')
        })
        expect(ensureRefundMock).toHaveBeenCalledWith(
            expect.objectContaining({
                sourceFactId: 'src_3',
                amount: '50.00',
                reason: '退差额',
            }),
        )
        expect(mutateAsyncMock).not.toHaveBeenCalled()
        expect(openRefundPreview).toHaveBeenCalledWith('crf-1')
        expect(result.current.refundSubmitOpen).toBe(true)
        expect(result.current.reverseConfirm).toBeNull()

        await act(async () => {
            await result.current.confirmRefundSubmit()
        })
        expect(submitRefundMock).toHaveBeenCalledWith({
            refundId: 'crf-1',
            expectedVersion: 1,
            idempotencyKey: expect.stringMatching(/^w11-rev-src_3-/),
        })
        expect(setLastResult).toHaveBeenCalledWith({
            status: 'succeeded',
            title: '退款已提交审批',
            description: '已按已绑定的审批流程启动审批，原回款保留。',
            reference: expect.stringMatching(/^w11-rev-src_3-/),
            facts: [
                { label: '退款单号', value: 'TK-1' },
                { label: '当前状态', value: '审批中' },
            ],
        })
        expect(setActionError).not.toHaveBeenCalled()
        expect(result.current.refundSubmitOpen).toBe(false)
    })

    it('reports success, clears all fields and closes the preview', async () => {
        mutateAsyncMock.mockResolvedValue({
            status: 'succeeded',
            reverseFactId: 'rf_3',
            reverseFactNo: 'RF-3',
            operationId: 'op_3',
            message: '反向记录已记录',
        })
        const { result, closePreview, setLastResult, setActionError } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'red_invoice',
                sourceFactId: 'src_3',
                label: 'INV-3',
                amount: '50.00',
            })
            result.current.setReverseReason('冲红')
            result.current.setReverseAmount('50.00')
        })
        await act(async () => {
            await result.current.confirmReverse()
        })
        expect(setLastResult).toHaveBeenCalledWith({
            status: 'succeeded',
            title: '反向记录已追加',
            description: '反向记录已记录',
            reference: 'op_3',
            facts: [
                { label: '反向单号', value: 'RF-3' },
                { label: '原记录', value: 'INV-3' },
            ],
        })
        expect(closePreview).toHaveBeenCalledTimes(1)
        expect(setActionError).not.toHaveBeenCalled()
        expect(result.current.reverseConfirm).toBeNull()
        expect(result.current.reverseReason).toBe('')
        expect(result.current.reverseAmount).toBe('')
    })

    it('reports an unknown outcome and keeps reason/amount entered', async () => {
        mutateAsyncMock.mockResolvedValue({
            status: 'unknown',
            message: '结果待确认',
            idempotencyKey: 'w11-rev-src_4-123',
        })
        const { result, setLastResult } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'red_invoice',
                sourceFactId: 'src_4',
                label: 'INV-4',
                amount: '90.00',
            })
            result.current.setReverseReason('冲红')
            result.current.setReverseAmount('90.00')
        })
        await act(async () => {
            await result.current.confirmReverse()
        })
        expect(setLastResult).toHaveBeenCalledWith({
            status: 'unknown',
            title: '纠错结果不确定',
            description: '结果待确认',
            reference: 'w11-rev-src_4-123',
        })
        expect(result.current.reverseConfirm).toBeNull()
        expect(result.current.reverseReason).toBe('冲红')
        expect(result.current.reverseAmount).toBe('90.00')
    })

    it('shows the failure message and closes the dialog', async () => {
        mutateAsyncMock.mockResolvedValue({
            status: 'failed',
            code: 'CONFLICT',
            message: '原记录已变更，请刷新后重试',
        })
        const { result, setActionError, setLastResult } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'red_invoice',
                sourceFactId: 'src_5',
                label: 'INV-5',
            })
        })
        await act(async () => {
            await result.current.confirmReverse()
        })
        expect(setActionError).toHaveBeenCalledWith(
            '原记录已变更，请刷新后重试',
        )
        expect(setLastResult).not.toHaveBeenCalled()
        expect(result.current.reverseConfirm).toBeNull()
    })

    it('reuses the refund key for the same receipt and reason retry', async () => {
        ensureRefundMock.mockResolvedValue({
            status: 'succeeded',
            refund: refundRow(),
        })
        const { result } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'refund',
                sourceFactId: 'src_3',
                label: 'RCP-3',
                amount: '50.00',
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft('退差额')
        })
        const firstKey = ensureRefundMock.mock.calls[0][0].idempotencyKey
        act(() => {
            result.current.setReverseConfirm({
                kind: 'refund',
                sourceFactId: 'src_3',
                label: 'RCP-3',
                amount: '50.00',
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft('退差额')
        })
        expect(ensureRefundMock.mock.calls[1][0].idempotencyKey).toBe(firstKey)
        expect(ensureRefundMock.mock.calls[1][0].sourceFactId).toBe('src_3')
    })

    it('rotates the refund key when the source receipt or reason changes', async () => {
        ensureRefundMock
            .mockResolvedValueOnce({
                status: 'succeeded',
                refund: refundRow({ refundId: 'crf-1', originalReceiptId: 'src_3' }),
            })
            .mockResolvedValueOnce({
                status: 'succeeded',
                refund: refundRow({
                    refundId: 'crf-2',
                    refundNo: 'TK-2',
                    originalReceiptId: 'src_9',
                }),
            })
            .mockResolvedValueOnce({
                status: 'succeeded',
                refund: refundRow({
                    refundId: 'crf-3',
                    refundNo: 'TK-3',
                    originalReceiptId: 'src_9',
                    reasonText: '全额退',
                }),
            })
        const { result } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'refund',
                sourceFactId: 'src_3',
                label: 'RCP-3',
                amount: '50.00',
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft('退差额')
        })
        const firstKey = ensureRefundMock.mock.calls[0][0].idempotencyKey

        act(() => {
            result.current.setReverseConfirm({
                kind: 'refund',
                sourceFactId: 'src_9',
                label: 'RCP-9',
                amount: '80.00',
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft('退差额')
        })
        const secondKey = ensureRefundMock.mock.calls[1][0].idempotencyKey
        expect(secondKey).not.toBe(firstKey)
        expect(ensureRefundMock.mock.calls[1][0].sourceFactId).toBe('src_9')

        act(() => {
            result.current.setReverseConfirm({
                kind: 'refund',
                sourceFactId: 'src_9',
                label: 'RCP-9',
                amount: '80.00',
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft('全额退')
        })
        expect(ensureRefundMock.mock.calls[2][0].idempotencyKey).not.toBe(
            secondKey,
        )
        expect(ensureRefundMock.mock.calls[2][0].reason).toBe('全额退')
    })

    it('keeps the prepare key when submitting the same draft', async () => {
        ensureRefundMock.mockResolvedValue({
            status: 'succeeded',
            refund: refundRow(),
        })
        submitRefundMock.mockResolvedValue({
            status: 'succeeded',
            refund: refundRow({
                status: 'in_approval',
                statusLabel: '审批中',
            }),
        })
        const { result } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'refund',
                sourceFactId: 'src_3',
                label: 'RCP-3',
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft('退差额')
        })
        const prepareKey = ensureRefundMock.mock.calls[0][0].idempotencyKey
        act(() => {
            result.current.beginRefundSubmit(refundRow())
        })
        await act(async () => {
            await result.current.confirmRefundSubmit()
        })
        expect(submitRefundMock).toHaveBeenCalledWith({
            refundId: 'crf-1',
            expectedVersion: 1,
            idempotencyKey: prepareKey,
        })
    })

    it('does not reuse the first refund key when submitting another draft', async () => {
        ensureRefundMock.mockResolvedValue({
            status: 'succeeded',
            refund: refundRow(),
        })
        submitRefundMock.mockResolvedValue({
            status: 'succeeded',
            refund: refundRow({
                refundId: 'crf-9',
                status: 'in_approval',
                statusLabel: '审批中',
            }),
        })
        const { result } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'refund',
                sourceFactId: 'src_3',
                label: 'RCP-3',
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft('退差额')
        })
        const firstKey = ensureRefundMock.mock.calls[0][0].idempotencyKey
        act(() => {
            result.current.beginRefundSubmit(
                refundRow({
                    refundId: 'crf-9',
                    refundNo: 'TK-9',
                    originalReceiptId: 'src_9',
                    reasonText: '另一笔',
                }),
            )
        })
        await act(async () => {
            await result.current.confirmRefundSubmit()
        })
        expect(submitRefundMock).toHaveBeenCalledWith({
            refundId: 'crf-9',
            expectedVersion: 1,
            idempotencyKey: expect.stringMatching(/^w11-rev-crf-9-/),
        })
        expect(submitRefundMock.mock.calls[0][0].idempotencyKey).not.toBe(
            firstKey,
        )
    })

    it('reuses the reversal key for the same receipt and reason retry', async () => {
        ensureReversalMock.mockResolvedValue({
            status: 'succeeded',
            reversal: reversalRow(),
        })
        const { result } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'receipt_reverse',
                sourceFactId: 'src_2',
                label: 'RCP-2',
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft('录入错误')
        })
        const firstKey = ensureReversalMock.mock.calls[0][0].idempotencyKey
        act(() => {
            result.current.setReverseConfirm({
                kind: 'receipt_reverse',
                sourceFactId: 'src_2',
                label: 'RCP-2',
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft('录入错误')
        })
        expect(ensureReversalMock.mock.calls[1][0].idempotencyKey).toBe(firstKey)
        expect(ensureReversalMock.mock.calls[1][0].sourceFactId).toBe('src_2')
    })

    it('rotates the reversal key when the source receipt or reason changes', async () => {
        ensureReversalMock
            .mockResolvedValueOnce({
                status: 'succeeded',
                reversal: reversalRow({ originalReceiptId: 'src_2' }),
            })
            .mockResolvedValueOnce({
                status: 'succeeded',
                reversal: reversalRow({
                    reversalId: 'rr-2',
                    reversalNo: 'CZ-2',
                    originalReceiptId: 'src_9',
                }),
            })
            .mockResolvedValueOnce({
                status: 'succeeded',
                reversal: reversalRow({
                    reversalId: 'rr-3',
                    reversalNo: 'CZ-3',
                    originalReceiptId: 'src_9',
                    reasonText: '金额错误',
                }),
            })
        const { result } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'receipt_reverse',
                sourceFactId: 'src_2',
                label: 'RCP-2',
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft('录入错误')
        })
        const firstKey = ensureReversalMock.mock.calls[0][0].idempotencyKey

        act(() => {
            result.current.setReverseConfirm({
                kind: 'receipt_reverse',
                sourceFactId: 'src_9',
                label: 'RCP-9',
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft('录入错误')
        })
        const secondKey = ensureReversalMock.mock.calls[1][0].idempotencyKey
        expect(secondKey).not.toBe(firstKey)
        expect(ensureReversalMock.mock.calls[1][0].sourceFactId).toBe('src_9')

        act(() => {
            result.current.setReverseConfirm({
                kind: 'receipt_reverse',
                sourceFactId: 'src_9',
                label: 'RCP-9',
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft('金额错误')
        })
        expect(ensureReversalMock.mock.calls[2][0].idempotencyKey).not.toBe(
            secondKey,
        )
        expect(ensureReversalMock.mock.calls[2][0].reason).toBe('金额错误')
    })

    it('does nothing when no reverse request is pending', async () => {
        const { result, setLastResult, setActionError } = setup()
        await act(async () => {
            await result.current.confirmReverse()
        })
        expect(mutateAsyncMock).not.toHaveBeenCalled()
        expect(setLastResult).not.toHaveBeenCalled()
        expect(setActionError).not.toHaveBeenCalled()
    })
})
