import { describe, it, expect } from 'vitest'
import { renderHook } from '@testing-library/react'

import type { SalesOrderDraftResumeData } from '@/features/sales-orders/api/sales-orders'
import type { SalesOrderDraftLineInput } from '@/features/sales-orders/types'
import { useSalesOrderCreateDefaults } from './use-sales-order-create-defaults'

const draftLine = (overrides: Partial<SalesOrderDraftLineInput> = {}): SalesOrderDraftLineInput => ({
    rowKey: 'draft-line-1',
    name: '',
    sku: '',
    skuRevisionId: '',
    quantity: '1',
    unit: '',
    unitPriceGross: '0.00',
    fulfillmentMode: '公司仓发',
    dueDate: '',
    faceValue: '',
    giftRate: '',
    cardForm: '',
    ...overrides,
})

const draft = (overrides: Partial<SalesOrderDraftResumeData> = {}): SalesOrderDraftResumeData => ({
    salesOrderId: 'so-1',
    documentNumber: 'SO-2026-001',
    version: 3,
    contractId: 'ct-1',
    nature: 'physical_service',
    welfareScene: 'annual',
    paymentTerms: 'POSTPAY_NET30',
    fulfillmentDeadline: '2026-09-30',
    targetMallId: '',
    receivableDueDate: '',
    taxRatePercent: '13.00',
    remark: '备注',
    lineItems: [draftLine({ name: '货物', sku: 'sku-1', quantity: '2', unitPriceGross: '10.00' })],
    ...overrides,
})

const baseInput = {
    initialCustomerId: 'cu-1',
    initialContractId: 'ct-9',
    initialContractRevisionId: 'r-9',
    initialNature: 'physical_service' as const,
    initialDraft: null,
}

describe('useSalesOrderCreateDefaults', () => {
    it('derives new-order defaults from the initial props', () => {
        const { result } = renderHook(() => useSalesOrderCreateDefaults(baseInput))

        const values = result.current
        expect(values.contractId).toBe('ct-9')
        expect(values.customerId).toBe('cu-1')
        expect(values.requestedContractRevisionId).toBe('r-9')
        expect(values.nature).toBe('physical_service')
        expect(values.taxRatePercent).toBe('13.00')
        expect(values.contractRevisionLabel).toBe('')
        expect(values.ownerUserId).toBe('')
        expect(values.lineItems).toHaveLength(1)
        expect(values.lineItems[0].unit).toBe('')
        expect(values.lineItems[0].fulfillmentMode).toBe('公司仓发')
        expect(values.lineItems[0].cardForm).toBe('')
    })

    it('uses the 6% tax default and card-specific line defaults for card vouchers', () => {
        const { result } = renderHook(() =>
            useSalesOrderCreateDefaults({
                ...baseInput,
                initialNature: 'card_voucher',
            }),
        )

        const values = result.current
        expect(values.nature).toBe('card_voucher')
        expect(values.taxRatePercent).toBe('6.00')
        expect(values.lineItems[0].unit).toBe('张')
        expect(values.lineItems[0].cardForm).toBe('电子卡')
        expect(values.lineItems[0].fulfillmentMode).toBe('')
    })

    it('keeps the same object while inputs are unchanged', () => {
        const { result, rerender } = renderHook(
            (props: typeof baseInput) => useSalesOrderCreateDefaults(props),
            { initialProps: baseInput },
        )

        const first = result.current
        rerender({ ...baseInput })
        expect(result.current).toBe(first)
    })

    it('recomputes when the initial contract changes', () => {
        const { result, rerender } = renderHook(
            (props: typeof baseInput) => useSalesOrderCreateDefaults(props),
            { initialProps: baseInput },
        )

        const first = result.current
        rerender({ ...baseInput, initialContractId: 'ct-10' })
        expect(result.current).not.toBe(first)
        expect(result.current.contractId).toBe('ct-10')
    })

    it('resumes draft content including its line items', () => {
        const { result } = renderHook(() =>
            useSalesOrderCreateDefaults({
                ...baseInput,
                initialDraft: draft(),
            }),
        )

        const values = result.current
        expect(values.contractId).toBe('ct-1')
        expect(values.nature).toBe('physical_service')
        expect(values.paymentTerms).toBe('POSTPAY_NET30')
        expect(values.fulfillmentDeadline).toBe('2026-09-30')
        expect(values.taxRatePercent).toBe('13.00')
        expect(values.remark).toBe('备注')
        expect(values.lineItems).toEqual(draft().lineItems)
    })

    it('falls back to a fresh empty line when the draft has no lines', () => {
        const { result } = renderHook(() =>
            useSalesOrderCreateDefaults({
                ...baseInput,
                initialDraft: draft({ lineItems: [], nature: 'card_voucher' }),
            }),
        )

        const values = result.current
        expect(values.nature).toBe('card_voucher')
        expect(values.lineItems).toHaveLength(1)
        expect(values.lineItems[0].unit).toBe('张')
    })
})
