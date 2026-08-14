import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook } from '@testing-library/react'

import { useSalesOrderDetailModeGuard } from '@/features/sales-orders/hooks/use-sales-order-detail-mode-guard'
import type { SalesOrderDetailView } from '@/features/sales-orders/api/sales-orders'

function makeOrder(
    overrides: Partial<SalesOrderDetailView> = {},
): SalesOrderDetailView {
    return {
        id: 'so-1',
        documentNumber: 'XS-1',
        primaryStatus: { code: 'effective', label: '已生效', tone: 'success' },
        procurementRejection: null,
        ...overrides,
    } as unknown as SalesOrderDetailView
}

const resubmittableRejection = {
    reviewStatus: 'REJECTED',
    allowedActions: ['RESUBMIT_CHANGED_TERMS'],
} as unknown as SalesOrderDetailView['procurementRejection']

describe('useSalesOrderDetailModeGuard', () => {
    let replaceOrderHref: ReturnType<
        typeof vi.fn<(patch: { section?: string; mode?: string | null }) => void>
    >

    beforeEach(() => {
        replaceOrderHref = vi.fn()
    })

    it('clears mode=edit when the order is neither draft nor resubmittable', () => {
        const order = makeOrder()
        renderHook(() =>
            useSalesOrderDetailModeGuard({
                order,
                pageMode: 'edit',
                replaceOrderHref,
            }),
        )

        expect(replaceOrderHref).toHaveBeenCalledWith({ mode: null })
    })

    it('keeps mode=edit for a draft order', () => {
        const order = makeOrder({
            primaryStatus: { code: 'draft', label: '草稿', tone: 'neutral' },
        })
        renderHook(() =>
            useSalesOrderDetailModeGuard({
                order,
                pageMode: 'edit',
                replaceOrderHref,
            }),
        )

        expect(replaceOrderHref).not.toHaveBeenCalled()
    })

    it('keeps mode=edit when the rejection allows resubmit', () => {
        const order = makeOrder({
            procurementRejection: resubmittableRejection,
        })
        renderHook(() =>
            useSalesOrderDetailModeGuard({
                order,
                pageMode: 'edit',
                replaceOrderHref,
            }),
        )

        expect(replaceOrderHref).not.toHaveBeenCalled()
    })

    it('does nothing when mode is not edit', () => {
        const order = makeOrder()
        renderHook(() =>
            useSalesOrderDetailModeGuard({
                order,
                pageMode: null,
                replaceOrderHref,
            }),
        )

        expect(replaceOrderHref).not.toHaveBeenCalled()
    })

    it('does nothing while the order is still loading', () => {
        renderHook(() =>
            useSalesOrderDetailModeGuard({
                order: undefined,
                pageMode: 'edit',
                replaceOrderHref,
            }),
        )

        expect(replaceOrderHref).not.toHaveBeenCalled()
    })
})
