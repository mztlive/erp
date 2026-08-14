import { describe, it, expect } from 'vitest'

import { parseView } from '@/features/supplier-payables/lib/url-state'

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
