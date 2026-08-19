import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act } from '@testing-library/react'

import { renderHookWithProviders } from '@/features/test-utils'
import { useReverseFlow } from './use-reverse-flow'
import type { CustomerRefundRow } from '@/features/customer-receivables/types'

const { mutateAsyncMock, ensureRefundMock, submitRefundMock } = vi.hoisted(() => ({
    mutateAsyncMock: vi.fn(),
    ensureRefundMock: vi.fn(),
    submitRefundMock: vi.fn(),
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

function setup() {
    const closePreview = vi.fn()
    const openRefundPreview = vi.fn()
    const setLastResult = vi.fn()
    const setActionError = vi.fn()
    const args = { closePreview, openRefundPreview, setLastResult, setActionError }
    const rendered = renderHookWithProviders(() => useReverseFlow(args))
    return { ...rendered, args, closePreview, openRefundPreview, setLastResult, setActionError }
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

    it('submits a receipt reverse without amount and with an explicit reason', async () => {
        mutateAsyncMock.mockResolvedValue({
            status: 'succeeded',
            reverseFactId: 'rf_2',
            reverseFactNo: 'RF-2',
            operationId: 'op_2',
            message: '已记录',
        })
        const { result } = setup()
        act(() => {
            result.current.setReverseConfirm({
                kind: 'receipt_reverse',
                sourceFactId: 'src_2',
                label: 'RCP-2',
            })
            result.current.setReverseReason('录入错误')
        })
        await act(async () => {
            await result.current.confirmReverse()
        })
        expect(mutateAsyncMock).toHaveBeenCalledWith(
            expect.objectContaining({
                kind: 'receipt_reverse',
                sourceFactId: 'src_2',
                amount: undefined,
                reason: '录入错误',
            }),
        )
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
            idempotencyKey: expect.stringMatching(/^w11-rev-src_3-\d+$/),
        })
        expect(setLastResult).toHaveBeenCalledWith({
            status: 'succeeded',
            title: '退款已提交审批',
            description: '已按已绑定的审批流程启动审批，原回款保留。',
            reference: expect.stringMatching(/^w11-rev-src_3-\d+$/),
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
                kind: 'receipt_reverse',
                sourceFactId: 'src_3',
                label: 'RCP-3',
                amount: '50.00',
            })
            result.current.setReverseReason('退差额')
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
                { label: '原记录', value: 'RCP-3' },
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
                kind: 'receipt_reverse',
                sourceFactId: 'src_5',
                label: 'RCP-5',
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
