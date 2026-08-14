import { describe, it, expect, afterEach } from 'vitest'
import { renderHook, act, cleanup } from '@testing-library/react'

import { useCardFundsReviewForms } from './use-card-funds-review-forms'
import { makeTask } from './test-data'

afterEach(() => {
    cleanup()
})

describe('useCardFundsReviewForms', () => {
    it('starts with empty drafts when there is no task', () => {
        const { result } = renderHook(() => useCardFundsReviewForms(undefined))
        expect(result.current.evidenceRef).toBe('')
        expect(result.current.evidenceDocId).toBe('')
        expect(result.current.comment).toBe('')
        expect(result.current.evidenceOk).toBe(false)
        expect(result.current.receiptForm).toEqual({
            receiptNo: '',
            receivedAt: '2026-07-01',
            grossAmount: '',
        })
        expect(result.current.invoiceForm).toEqual({
            invoiceNo: '',
            issuedAt: '2026-07-01',
            grossAmount: '',
            netAmount: '',
            taxAmount: '',
        })
        expect(result.current.allocLines).toEqual([])
        expect(result.current.allocationMode).toBeNull()
    })

    it('hydrates evidence drafts from the task currentEvidence', () => {
        const task = makeTask({
            currentEvidence: {
                evidenceDocumentIds: ['doc_1'],
                evidenceReferences: ['ref_1'],
                comment: '已有备注',
            },
        })
        const { result } = renderHook(() => useCardFundsReviewForms(task))
        expect(result.current.evidenceDocId).toBe('doc_1')
        expect(result.current.evidenceRef).toBe('ref_1')
        expect(result.current.comment).toBe('已有备注')
        expect(result.current.evidenceOk).toBe(true)
        expect(result.current.evidenceDirty).toBe(false)
    })

    it('resets all drafts when switching to another task', () => {
        const first = makeTask({
            currentEvidence: {
                evidenceDocumentIds: ['doc_1'],
                evidenceReferences: [],
                comment: '备注A',
            },
        })
        const second = makeTask({
            workItem: { workItemId: 'wi_2' },
            currentEvidence: {
                evidenceDocumentIds: ['doc_2'],
                evidenceReferences: [],
                comment: '备注B',
            },
        })
        const { result, rerender } = renderHook(
            ({ task }: { task: ReturnType<typeof makeTask> }) =>
                useCardFundsReviewForms(task),
            { initialProps: { task: first } },
        )
        expect(result.current.evidenceDocId).toBe('doc_1')

        act(() => {
            result.current.setEvidenceDocId('用户改动')
            result.current.setEvidenceDirty(true)
            result.current.setAllocationMode('receipt')
            result.current.setAllocLines([
                {
                    lineId: 'al_1',
                    targetAccountId: 'acct_1',
                    targetLabel: 'SO-1',
                    amount: '10.00',
                },
            ])
        })
        expect(result.current.evidenceDocId).toBe('用户改动')
        expect(result.current.allocationMode).toBe('receipt')

        rerender({ task: second })
        expect(result.current.evidenceDocId).toBe('doc_2')
        expect(result.current.comment).toBe('备注B')
        expect(result.current.evidenceDirty).toBe(false)
        expect(result.current.allocationMode).toBeNull()
        expect(result.current.allocLines).toEqual([])
        expect(result.current.receiptForm.grossAmount).toBe('')
    })

    it('derives evidenceOk from either field', () => {
        const task = makeTask()
        const { result } = renderHook(() => useCardFundsReviewForms(task))
        expect(result.current.evidenceOk).toBe(false)

        act(() => {
            result.current.setEvidenceRef('  银行回单  ')
        })
        expect(result.current.evidenceOk).toBe(true)

        act(() => {
            result.current.setEvidenceRef('')
        })
        expect(result.current.evidenceOk).toBe(false)

        act(() => {
            result.current.setEvidenceDocId('DOC-1')
        })
        expect(result.current.evidenceOk).toBe(true)
    })

    it('openAllocation seeds the first line with the draft gross amount', () => {
        const task = makeTask()
        const { result } = renderHook(() => useCardFundsReviewForms(task))

        act(() => {
            result.current.setReceiptForm((f) => ({
                ...f,
                grossAmount: '88.00',
            }))
        })
        act(() => {
            result.current.openAllocation('receipt')
        })
        expect(result.current.allocationMode).toBe('receipt')
        expect(result.current.allocLines).toEqual([
            {
                lineId: 'al_1',
                targetAccountId: 'acct_1',
                targetLabel: 'SO-2026-001 · 演示客户',
                amount: '88.00',
            },
        ])

        act(() => {
            result.current.setInvoiceForm((f) => ({
                ...f,
                grossAmount: '113.00',
            }))
        })
        act(() => {
            result.current.openAllocation('invoice')
        })
        expect(result.current.allocationMode).toBe('invoice')
        expect(result.current.allocLines).toEqual([
            {
                lineId: 'al_1',
                targetAccountId: 'acct_1',
                targetLabel: 'SO-2026-001 · 演示客户',
                amount: '113.00',
            },
        ])
    })

    it('openAllocation keeps the first line when the amount is empty', () => {
        const task = makeTask()
        const { result } = renderHook(() => useCardFundsReviewForms(task))
        act(() => {
            result.current.openAllocation('receipt')
        })
        expect(result.current.allocLines[0]?.amount).toBe('0.00')
    })

    it('openAllocation is a no-op without a task', () => {
        const { result } = renderHook(() => useCardFundsReviewForms(undefined))
        act(() => {
            result.current.openAllocation('receipt')
        })
        expect(result.current.allocationMode).toBeNull()
        expect(result.current.allocLines).toEqual([])
    })
})
