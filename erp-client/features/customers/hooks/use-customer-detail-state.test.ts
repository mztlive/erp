import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, renderHook } from '@testing-library/react'

import { useCustomerDetailState } from './use-customer-detail-state'
import type { CustomerCenterView } from '@/features/customers/types'

const navMocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    back: vi.fn(),
}))

vi.mock('next/navigation', () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: navMocks.back,
    }),
}))

const customerMocks = vi.hoisted(() => ({
    data: null as CustomerCenterView | null,
    refetch: vi.fn(),
}))

vi.mock('@/features/customers/hooks/queries', () => ({
    useCustomerCenterQuery: () => ({
        isPending: false,
        isError: false,
        data: customerMocks.data,
        refetch: customerMocks.refetch,
    }),
}))

function makeCustomer(
    overrides: Partial<CustomerCenterView> = {},
): CustomerCenterView {
    return {
        customerId: 'cust-1',
        partyId: 'party-1',
        customerNo: 'C-001',
        status: 'active',
        statusLabel: { label: '启用', tone: 'success' },
        lockVersion: 3,
        partyLockVersion: 2,
        currentRevision: {
            revisionId: 'r2',
            revisionNo: 2,
            legalName: '示例贸易有限公司',
            shortName: '示例贸易',
            unifiedCreditCode: '91310000XXXXXXXXXX',
            defaultPaymentTerm: 'POSTPAY_NET30',
            effectiveFrom: '2026-06-01T00:00:00.000Z',
        },
        assignments: [],
        contacts: [],
        addresses: [],
        bankAccounts: [],
        metrics: {
            activeContractCount: 0,
            inProgressSalesOrderCount: 0,
            receivableBalance: null,
            overdueAmount: null,
        },
        contracts: [],
        salesOrders: [],
        freshness: { formalFactsAt: '2026-06-01T00:00:00.000Z' },
        allowedActions: ['EDIT_CUSTOMER'],
        actionBlockers: [],
        revisionTimeline: [],
        partitions: {
            identity: 'ok',
            contacts: 'ok',
            related: 'ok',
            settlement: 'ok',
            quality: 'ok',
            audit: 'ok',
        },
        ...overrides,
    }
}

describe('useCustomerDetailState', () => {
    beforeEach(() => {
        customerMocks.data = null
        customerMocks.refetch = vi.fn()
        navMocks.replace.mockClear()
        navMocks.push.mockClear()
    })

    it('resolves the active section with overview as fallback', () => {
        const { result: missing } = renderHook(() =>
            useCustomerDetailState('cust-1'),
        )
        expect(missing.current.activeSection).toBe('overview')

        const { result: invalid } = renderHook(() =>
            useCustomerDetailState('cust-1', 'nope'),
        )
        expect(invalid.current.activeSection).toBe('overview')

        const { result: valid } = renderHook(() =>
            useCustomerDetailState('cust-1', 'settlement'),
        )
        expect(valid.current.activeSection).toBe('settlement')
    })

    it('navigates between sections via router.replace without scrolling', () => {
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'overview'),
        )

        act(() => result.current.handleSectionChange('related'))

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/customers/cust-1?section=related',
            { scroll: false },
        )
    })

    it('navigates back to the bare url when the overview section is chosen', () => {
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'related'),
        )

        act(() => result.current.handleSectionChange('overview'))
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/customers/cust-1',
            { scroll: false },
        )
    })

    it('ignores switching to the section that is already active', () => {
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'overview'),
        )

        act(() => result.current.handleSectionChange('overview'))

        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('defers switching behind a discard confirmation while editing with unsaved input', () => {
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'overview'),
        )

        act(() => {
            result.current.startEditing()
            result.current.setFormDirty(true)
        })
        act(() => result.current.handleSectionChange('related'))

        expect(navMocks.replace).not.toHaveBeenCalled()
        expect(result.current.pendingSection).toBe('related')
        expect(result.current.editing).toBe(true)
    })

    it('switches immediately while editing when the form is clean', () => {
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'overview'),
        )

        act(() => result.current.startEditing())
        act(() => result.current.handleSectionChange('related'))

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/customers/cust-1?section=related',
            { scroll: false },
        )
        expect(result.current.pendingSection).toBeNull()
    })

    it('discarding the pending switch exits editing and navigates', () => {
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'overview'),
        )

        act(() => {
            result.current.startEditing()
            result.current.setFormDirty(true)
        })
        act(() => result.current.handleSectionChange('settlement'))
        act(() => result.current.discardPendingAndSwitch())

        expect(navMocks.replace).toHaveBeenCalledWith(
            '/sales/customers/cust-1?section=settlement',
            { scroll: false },
        )
        expect(result.current.pendingSection).toBeNull()
        expect(result.current.editing).toBe(false)
        expect(result.current.formDirty).toBe(false)
    })

    it('cancelling editing resets editing and dirty state', () => {
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'overview'),
        )

        act(() => {
            result.current.startEditing()
            result.current.setFormDirty(true)
        })
        expect(result.current.editing).toBe(true)
        expect(result.current.formDirty).toBe(true)

        act(() => result.current.cancelEditing())
        expect(result.current.editing).toBe(false)
        expect(result.current.formDirty).toBe(false)
    })

    it('completing editing records the saved notice with the revision number', () => {
        customerMocks.data = makeCustomer()
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'overview'),
        )

        act(() => {
            result.current.startEditing()
            result.current.setFormDirty(true)
        })
        act(() => result.current.completeEditing(5))

        expect(result.current.editing).toBe(false)
        expect(result.current.formDirty).toBe(false)
        expect(result.current.savedNotice).toEqual({ revisionNo: 5 })
    })

    it('falls back to the current revision number when none is reported', () => {
        customerMocks.data = makeCustomer()
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'overview'),
        )

        act(() => result.current.completeEditing())

        expect(result.current.savedNotice).toEqual({ revisionNo: 2 })
    })

    it('dismisses the saved notice and the pending section', () => {
        customerMocks.data = makeCustomer()
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'overview'),
        )

        act(() => result.current.completeEditing(3))
        expect(result.current.savedNotice).not.toBeNull()
        act(() => result.current.dismissSavedNotice())
        expect(result.current.savedNotice).toBeNull()

        act(() => {
            result.current.startEditing()
            result.current.setFormDirty(true)
        })
        act(() => result.current.handleSectionChange('quality'))
        act(() => result.current.dismissPendingSection())
        expect(result.current.pendingSection).toBeNull()
        expect(navMocks.replace).not.toHaveBeenCalled()
    })

    it('exposes the query state and customer data for the page', () => {
        const center = makeCustomer()
        customerMocks.data = center
        const { result } = renderHook(() =>
            useCustomerDetailState('cust-1', 'overview'),
        )

        expect(result.current.customer).toBe(center)
        expect(result.current.query.isPending).toBe(false)
        expect(result.current.query.refetch).toBe(customerMocks.refetch)
    })
})
