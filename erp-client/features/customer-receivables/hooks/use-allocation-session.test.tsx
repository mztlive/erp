import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, act, waitFor } from '@testing-library/react'

import { useAllocationSession } from '@/features/customer-receivables/hooks/use-allocation-session'
import type {
    AllocationSessionView,
    AllocationTarget,
    PostAllocationResult,
} from '@/features/customer-receivables/types'

const mutationMocks = vi.hoisted(() => ({
    save: { mutateAsync: vi.fn(), isPending: false },
    post: { mutateAsync: vi.fn(), isPending: false },
    resolve: { mutateAsync: vi.fn(), isPending: false },
}))

vi.mock('@/features/customer-receivables/hooks/queries', () => ({
    useSaveAllocationDraftMutation: () => mutationMocks.save,
    usePostAllocationMutation: () => mutationMocks.post,
    useResolvePostUnknownMutation: () => mutationMocks.resolve,
}))

const target = (overrides: Partial<AllocationTarget> = {}): AllocationTarget => ({
    targetId: 'e1',
    targetKind: 'receivable_entry',
    label: 'SO-1 · goods',
    salesOrderNo: 'SO-1',
    openAmount: '60',
    dueDate: '2026-02-01',
    counterpartyPartyId: 'p1',
    baselineVersion: 1,
    ...overrides,
})

const session = (overrides: Partial<AllocationSessionView> = {}): AllocationSessionView => ({
    draftSessionId: 'alloc_cust_1',
    mode: 'receipt',
    counterpartyPartyId: 'p1',
    counterpartyPartyName: '主体甲',
    customerId: 'c1',
    customerName: '客户甲',
    status: 'draft',
    fact: { receivedAt: '2026-01-01T10:00', amount: '100', bankReference: 'ref' },
    pool: [target()],
    allocations: [],
    proposedAllocatedTotal: '0.00',
    proposedUnallocated: '100.00',
    factAmount: '100',
    submitPolicy: {
        allowUnallocatedRemainder: true,
        label: '允许保留未分配余额（系统统一判定）',
    },
    leaseValid: true,
    editVersion: 1,
    note: '本次核销已锁定往来主体。',
    ...overrides,
})

const succeededPost = (): PostAllocationResult => ({
    status: 'succeeded',
    mode: 'receipt',
    factId: 'r1',
    factNo: 'SK-1',
    allocatedTotal: '60.00',
    unallocatedAmount: '40.00',
    operationId: 'op1',
    watermark: '2026-01-01T00:00:00.000Z',
})

function setup(overrides: Partial<AllocationSessionView> = {}) {
    const onClose = vi.fn()
    const onPosted = vi.fn()
    const { result, rerender } = renderHook(
        (props: { session: AllocationSessionView }) =>
            useAllocationSession({
                session: props.session,
                onClose,
                onPosted,
            }),
        {
            initialProps: { session: session(overrides) },
        },
    )
    return { result, rerender, onClose, onPosted }
}

describe('useAllocationSession', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('derives initial form values, allocations and totals from the session', () => {
        const seeded = session({
            fact: { receivedAt: '2026-01-01T10:00', amount: '100', bankReference: 'ref' },
        })
        const { result } = setup(seeded)

        expect(result.current.form.state.values).toMatchObject({
            receivedAt: '2026-01-01T10:00',
            amount: '100',
            bankReference: 'ref',
        })
        expect(result.current.allocations).toEqual([])
        expect(result.current.editVersion).toBe(1)
        expect(result.current.factAmountStr).toBe('100')
        expect(result.current.proposedUnallocated).toBe(100)
        expect(result.current.issues).toEqual([])
        expect(result.current.canSubmit).toBe(true)
        expect(result.current.locked).toBe(false)
        expect(result.current.existing).toBe(false)
        expect(result.current.isReceipt).toBe(true)
    })

    it('adds a pool target once and rejects cross-party targets', () => {
        const { result } = setup()

        act(() => result.current.addFromPool(target()))
        expect(result.current.allocations).toHaveLength(1)
        expect(result.current.allocations[0]).toMatchObject({
            targetId: 'e1',
            amount: '',
        })

        act(() => result.current.addFromPool(target()))
        expect(result.current.allocations).toHaveLength(1)

        act(() =>
            result.current.addFromPool(
                target({ targetId: 'e2', counterpartyPartyId: 'p-other' }),
            ),
        )
        expect(result.current.allocations).toHaveLength(1)
        expect(result.current.actionError).toBe(
            '跨主体目标不能分配，请选择同主体目标。',
        )
    })

    it('updates and removes allocation amounts', () => {
        const { result } = setup()
        act(() => result.current.addFromPool(target()))
        const lineKey = result.current.allocations[0].lineKey

        act(() => result.current.updateAmount(lineKey, '12.5'))
        expect(result.current.allocations[0].amount).toBe('12.5')

        act(() => result.current.removeLine(lineKey))
        expect(result.current.allocations).toEqual([])
    })

    it('fillLineAmount caps at the remaining fact amount and open balance', () => {
        const { result } = setup()
        act(() => result.current.addFromPool(target()))
        act(() =>
            result.current.addFromPool(
                target({ targetId: 'e2', openAmount: '30' }),
            ),
        )

        act(() => result.current.fillLineAmount(result.current.allocations[1]))
        // remaining = 100 - 0 → min(open 30, 100)
        expect(result.current.allocations[1].amount).toBe('30.00')

        act(() => result.current.fillLineAmount(result.current.allocations[0]))
        // remaining = 100 - 30 → min(open 60, 70)
        expect(result.current.allocations[0].amount).toBe('60.00')
    })

    it('computes validation issues for negative, over-open and over-fact amounts', () => {
        const { result } = setup()
        act(() => result.current.addFromPool(target()))
        const lineKey = result.current.allocations[0].lineKey

        act(() => result.current.updateAmount(lineKey, '-5'))
        expect(
            result.current.issues.some((i) => i.id === `neg-${lineKey}`),
        ).toBe(true)

        act(() => result.current.updateAmount(lineKey, '200'))
        const issues = result.current.issues
        expect(issues.some((i) => i.id === `over-${lineKey}`)).toBe(true)
        expect(issues.some((i) => i.id === 'over-fact')).toBe(true)

        act(() => result.current.updateAmount(lineKey, '60'))
        expect(result.current.issues).toEqual([])
    })

    it('blocks submit for zero fact amount, invalid lease or posted session', () => {
        expect(setup().result.current.canSubmit).toBe(true)
        expect(
            setup({ fact: { receivedAt: '', amount: '', bankReference: '' } })
                .result.current.canSubmit,
        ).toBe(false)
        expect(setup({ leaseValid: false }).result.current.canSubmit).toBe(
            false,
        )
        expect(setup({ status: 'posted' }).result.current.canSubmit).toBe(
            false,
        )
        expect(setup({ status: 'posted' }).result.current.locked).toBe(true)
    })

    it('locks the form for existing facts', () => {
        const { result } = setup({ existingFactId: 'r1', existingFactNo: 'SK-9' })
        expect(result.current.locked).toBe(true)
    })

    it('saves the draft and applies the returned edit version', async () => {
        const next = { ...session(), editVersion: 2, savedAt: '2026-01-01T01:00:00.000Z' }
        mutationMocks.save.mutateAsync.mockResolvedValue(next)

        const { result } = setup()
        act(() => result.current.addFromPool(target()))
        const lineKey = result.current.allocations[0].lineKey
        act(() => result.current.updateAmount(lineKey, '10'))

        await act(async () => {
            await result.current.doSaveDraft()
        })

        expect(mutationMocks.save.mutateAsync).toHaveBeenCalledTimes(1)
        expect(mutationMocks.save.mutateAsync.mock.calls[0][0]).toEqual({
            draftSessionId: 'alloc_cust_1',
            fact: {
                receivedAt: '2026-01-01T10:00',
                amount: '100',
                bankReference: 'ref',
            },
            allocations: [
                expect.objectContaining({ targetId: 'e1', amount: '10' }),
            ],
            editVersion: 1,
        })
        expect(result.current.editVersion).toBe(2)
        expect(result.current.draftSavedAt).toBe('2026-01-01T01:00:00.000Z')
        expect(result.current.actionError).toBeNull()
    })

    it('surfaces the error message when saving the draft fails', async () => {
        mutationMocks.save.mutateAsync.mockRejectedValue(new Error('草稿已更新'))

        const { result } = setup()
        await act(async () => {
            await result.current.doSaveDraft()
        })

        expect(result.current.actionError).toBe('草稿已更新')
    })

    it('posts after saving and marks the session posted on success', async () => {
        mutationMocks.save.mutateAsync.mockResolvedValue({
            ...session(),
            editVersion: 2,
        })
        mutationMocks.post.mutateAsync.mockResolvedValue(succeededPost())

        const { result, onPosted } = setup()
        act(() => result.current.addFromPool(target()))
        act(() => result.current.updateAmount('line_e1_1', '60'))

        await act(async () => {
            await result.current.doPost()
        })

        expect(mutationMocks.post.mutateAsync).toHaveBeenCalledTimes(1)
        expect(mutationMocks.post.mutateAsync.mock.calls[0][0]).toEqual({
            draftSessionId: 'alloc_cust_1',
            editVersion: 2,
            idempotencyKey: expect.stringMatching(/^w11-post-alloc_cust_1-/),
        })
        expect(result.current.postedLocally).toBe(true)
        expect(result.current.locked).toBe(true)
        expect(result.current.confirmOpen).toBe(false)
        expect(result.current.result?.status).toBe('succeeded')
        expect(onPosted).toHaveBeenCalledTimes(1)
    })

    it('sets an actionable error and refreshes open amounts on balance conflict', async () => {
        mutationMocks.save.mutateAsync.mockResolvedValue({
            ...session(),
            editVersion: 2,
        })
        mutationMocks.post.mutateAsync.mockResolvedValue({
            status: 'failed',
            code: 'BALANCE_CONFLICT',
            message: '开放余额已变化',
            refreshedTargets: [{ targetId: 'e1', openAmount: '30' }],
        } satisfies PostAllocationResult)

        const { result } = setup()
        act(() => result.current.addFromPool(target()))

        await act(async () => {
            await result.current.doPost()
        })

        expect(result.current.actionError).toBe('开放余额已变化')
        expect(result.current.allocations[0].openAmount).toBe('30')
        expect(result.current.confirmOpen).toBe(false)
        expect(result.current.postedLocally).toBe(false)
    })

    it('reports a failed submit without posting state', async () => {
        mutationMocks.save.mutateAsync.mockRejectedValue(new Error('提交失败'))

        const { result } = setup()
        await act(async () => {
            await result.current.doPost()
        })

        expect(result.current.actionError).toBe('提交失败')
        expect(result.current.confirmOpen).toBe(false)
        expect(result.current.postedLocally).toBe(false)
    })

    it('resolves an unknown post result via the idempotency key', async () => {
        mutationMocks.save.mutateAsync.mockResolvedValue({
            ...session(),
            editVersion: 2,
        })
        mutationMocks.post.mutateAsync.mockResolvedValue({
            status: 'unknown',
            message: '结果未知',
            idempotencyKey: 'k-unknown',
            operationId: 'op-x',
        } satisfies PostAllocationResult)
        mutationMocks.resolve.mutateAsync.mockResolvedValue(succeededPost())

        const { result, onPosted } = setup()
        act(() => result.current.addFromPool(target()))
        act(() => result.current.updateAmount('line_e1_1', '60'))

        await act(async () => {
            await result.current.doPost()
        })
        expect(result.current.result?.status).toBe('unknown')
        expect(result.current.result?.pendingKey).toBe('k-unknown')

        await act(async () => {
            await result.current.resolveUnknown()
        })

        expect(mutationMocks.resolve.mutateAsync).toHaveBeenCalledTimes(1)
        expect(mutationMocks.resolve.mutateAsync.mock.calls[0][0]).toBe(
            'k-unknown',
        )
        expect(result.current.result?.status).toBe('succeeded')
        expect(onPosted).toHaveBeenCalledTimes(1)
    })

    it('requestClose closes directly when clean and confirms when dirty', () => {
        const { result, onClose } = setup()
        act(() => result.current.requestClose())
        expect(onClose).toHaveBeenCalledTimes(1)
        expect(result.current.leaveConfirmOpen).toBe(false)

        act(() => result.current.addFromPool(target()))
        act(() => result.current.requestClose())
        expect(onClose).toHaveBeenCalledTimes(1)
        expect(result.current.leaveConfirmOpen).toBe(true)

        act(() => result.current.setLeaveConfirmOpen(false))
        expect(result.current.leaveConfirmOpen).toBe(false)
    })

    it('requestClose closes without confirmation once posted', () => {
        const { result, onClose } = setup({ status: 'posted' })
        act(() => result.current.addFromPool(target()))
        act(() => result.current.requestClose())
        expect(onClose).toHaveBeenCalledTimes(1)
        expect(result.current.leaveConfirmOpen).toBe(false)
    })

    it('prefills net/tax at 13% when invoice gross changes and both are empty', async () => {
        const { result } = setup({ mode: 'invoice', fact: { invoiceNo: '', invoiceDate: '2026-01-01', grossAmount: '' } })

        act(() => {
            result.current.form.setFieldValue('grossAmount', '113')
        })
        // panel-level derived values react on the next host re-render
        act(() => {
            result.current.setConfirmOpen(true)
        })
        await waitFor(() =>
            expect(result.current.form.state.values.netAmount).toBe('100.00'),
        )
        expect(result.current.form.state.values.taxAmount).toBe('13.00')

        // existing net/tax are kept untouched on further gross edits
        act(() => {
            result.current.form.setFieldValue('grossAmount', '226')
        })
        act(() => {
            result.current.setConfirmOpen(false)
        })
        await waitFor(() =>
            expect(result.current.form.state.values.netAmount).toBe('100.00'),
        )
        expect(result.current.form.state.values.taxAmount).toBe('13.00')
    })

    it('derives fact amount from grossAmount in invoice mode', async () => {
        const { result } = setup({ mode: 'invoice', fact: { invoiceNo: '', invoiceDate: '2026-01-01', grossAmount: '200' } })

        expect(result.current.factAmountStr).toBe('200')

        act(() => {
            result.current.form.setFieldValue('grossAmount', '300')
        })
        act(() => {
            result.current.setConfirmOpen(true)
        })
        await waitFor(() => expect(result.current.factAmountStr).toBe('300'))
    })
})
