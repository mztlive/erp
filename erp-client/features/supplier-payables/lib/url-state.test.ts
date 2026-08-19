import { describe, it, expect } from 'vitest'

import {
    parsePreviewKind,
    parseView,
    parseWorkItemId,
} from '@/features/supplier-payables/lib/url-state'

describe('parseView', () => {
    it('defaults to payable for a missing param', () => {
        expect(parseView(null)).toBe('payable')
    })

    it('accepts each registered view value', () => {
        expect(parseView('payable')).toBe('payable')
        expect(parseView('payment')).toBe('payment')
        expect(parseView('purchase_invoice')).toBe('purchase_invoice')
        expect(parseView('unallocated')).toBe('unallocated')
    })

    it('falls back to payable for unknown values', () => {
        expect(parseView('unknown')).toBe('payable')
        expect(parseView('')).toBe('payable')
        expect(parseView('PAYMENT')).toBe('payable')
    })
})

describe('parsePreviewKind', () => {
    it('only promotes payment, refund or reversal when the query is exact', () => {
        expect(parsePreviewKind('payment')).toBe('payment')
        expect(parsePreviewKind('refund')).toBe('refund')
        expect(parsePreviewKind('reversal')).toBe('reversal')
        expect(parsePreviewKind('payable')).toBe('payable')
        expect(parsePreviewKind(null)).toBe('payable')
        expect(parsePreviewKind('invoice')).toBe('payable')
    })
})

describe('parseWorkItemId', () => {
    it('prefers currentWorkItemId and falls back to workItemId', () => {
        expect(
            parseWorkItemId(
                new URLSearchParams('currentWorkItemId=wi-1&workItemId=wi-2'),
            ),
        ).toBe('wi-1')
        expect(parseWorkItemId(new URLSearchParams('workItemId=wi-2'))).toBe(
            'wi-2',
        )
        expect(parseWorkItemId(new URLSearchParams('view=payment'))).toBeUndefined()
    })
})
