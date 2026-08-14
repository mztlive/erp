import { describe, it, expect, vi } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'

import type {
    CustomerAccountsListView,
    ReceivableAccountRow,
} from '@/features/customer-receivables/types'
import { useAutoAllocationSession } from './use-auto-allocation-session'

function makeListView(
    overrides: Partial<CustomerAccountsListView> = {},
): CustomerAccountsListView {
    return {
        view: 'receivable',
        metrics: {
            openReceivableTotal: '0',
            overdueReceivableTotal: '0',
            unallocatedReceiptTotal: '0',
            unallocatedInvoiceTotal: '0',
            cardPendingReviewCount: 0,
        },
        receivables: [],
        receipts: [],
        invoices: [],
        unallocated: { receipts: [], invoices: [], note: '' },
        counterparties: [],
        total: 0,
        filterSummary: '',
        permissionVersion: 'v1',
        dataWatermark: 'w',
        queriedAt: '',
        hasDataScope: true,
        moduleAllowed: true,
        canRegister: true,
        canExport: true,
        submitPolicy: {
            allowUnallocatedRemainder: false,
            label: '保留未分配余额',
        },
        ...overrides,
    }
}

function setup(overrides: Partial<Parameters<typeof useAutoAllocationSession>[0]> = {}) {
    const mutateAsync = vi.fn().mockResolvedValue({ draftSessionId: 'ses_1' })
    const patchUrl = vi.fn()
    const setActionError = vi.fn()
    const args = {
        data: makeListView(),
        from: 'W05',
        returnTo: '/orders/s1',
        sessionId: undefined,
        counterpartyPartyId: 'p1',
        customerId: undefined,
        salesOrderId: 'so_1',
        receivableAccountId: 'ra_1',
        createSession: { mutateAsync },
        patchUrl,
        setActionError,
        ...overrides,
    }
    const rendered = renderHook((props) => useAutoAllocationSession(props), {
        initialProps: args,
    })
    return { ...rendered, args, mutateAsync, patchUrl, setActionError }
}

describe('useAutoAllocationSession', () => {
    it('opens a receipt session when entering from W05 with a party param', async () => {
        const { mutateAsync, patchUrl, setActionError } = setup()
        await waitFor(() =>
            expect(mutateAsync).toHaveBeenCalledWith({
                mode: 'receipt',
                counterpartyPartyId: 'p1',
                salesOrderId: 'so_1',
                receivableAccountId: 'ra_1',
                returnTo: '/orders/s1',
                from: 'W05',
            }),
        )
        await waitFor(() =>
            expect(patchUrl).toHaveBeenCalledWith(
                { sessionId: 'ses_1' },
                { replace: true },
            ),
        )
        expect(setActionError).not.toHaveBeenCalled()
    })

    it('falls back to the first receivable counterparty when no party param', async () => {
        const data = makeListView({
            receivables: [
                { counterpartyPartyId: 'p2' } as ReceivableAccountRow,
            ],
        })
        const { mutateAsync } = setup({
            data,
            counterpartyPartyId: undefined,
        })
        await waitFor(() =>
            expect(mutateAsync).toHaveBeenCalledWith(
                expect.objectContaining({ counterpartyPartyId: 'p2' }),
            ),
        )
    })

    it('falls back to the locked customer counterparty', async () => {
        const data = makeListView({
            counterparties: [
                {
                    counterpartyPartyId: 'p3',
                    counterpartyPartyName: '主体三',
                    customerId: 'c3',
                    customerName: '客户三',
                },
            ],
        })
        const { mutateAsync } = setup({
            data,
            counterpartyPartyId: undefined,
            customerId: 'c3',
        })
        await waitFor(() =>
            expect(mutateAsync).toHaveBeenCalledWith(
                expect.objectContaining({ counterpartyPartyId: 'p3' }),
            ),
        )
    })

    it('does nothing when there is no party to allocate to', async () => {
        const { mutateAsync, patchUrl, setActionError } = setup({
            counterpartyPartyId: undefined,
            customerId: undefined,
        })
        await new Promise((resolve) => setTimeout(resolve, 0))
        expect(mutateAsync).not.toHaveBeenCalled()
        expect(patchUrl).not.toHaveBeenCalled()
        expect(setActionError).not.toHaveBeenCalled()
    })

    it('does nothing without the W05 entry context', async () => {
        for (const args of [
            { from: undefined },
            { returnTo: undefined },
            { from: 'W06' },
        ]) {
            const { mutateAsync } = setup(args)
            await new Promise((resolve) => setTimeout(resolve, 0))
            expect(mutateAsync).not.toHaveBeenCalled()
        }
    })

    it('does nothing when a session is already open', async () => {
        const { mutateAsync } = setup({ sessionId: 'ses_9' })
        await new Promise((resolve) => setTimeout(resolve, 0))
        expect(mutateAsync).not.toHaveBeenCalled()
    })

    it('does nothing while data is loading or register is not allowed', async () => {
        const { mutateAsync } = setup({ data: undefined })
        await new Promise((resolve) => setTimeout(resolve, 0))
        expect(mutateAsync).not.toHaveBeenCalled()

        const blocked = setup({ data: makeListView({ canRegister: false }) })
        await new Promise((resolve) => setTimeout(resolve, 0))
        expect(blocked.mutateAsync).not.toHaveBeenCalled()
    })

    it('runs the auto-open only once across re-renders', async () => {
        const { mutateAsync, args, rerender } = setup()
        await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
        rerender({ ...args, data: makeListView() })
        await new Promise((resolve) => setTimeout(resolve, 0))
        expect(mutateAsync).toHaveBeenCalledTimes(1)
    })

    it('reports the error when session creation fails', async () => {
        const mutateAsync = vi
            .fn()
            .mockRejectedValue(new Error('创建会话被拒绝'))
        const { setActionError } = setup({
            createSession: { mutateAsync },
        })
        await waitFor(() =>
            expect(setActionError).toHaveBeenCalledWith('创建会话被拒绝'),
        )
    })
})
