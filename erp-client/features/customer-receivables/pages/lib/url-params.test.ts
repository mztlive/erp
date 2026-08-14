import { describe, it, expect } from 'vitest'

import { parseDue, parseView } from './url-params'

describe('parseView', () => {
    it('accepts every known view value', () => {
        expect(parseView('receipt')).toBe('receipt')
        expect(parseView('sales_invoice')).toBe('sales_invoice')
        expect(parseView('unallocated')).toBe('unallocated')
        expect(parseView('receivable')).toBe('receivable')
    })

    it('falls back to receivable for missing or unknown values', () => {
        expect(parseView(null)).toBe('receivable')
        expect(parseView('')).toBe('receivable')
        expect(parseView('bogus')).toBe('receivable')
    })
})

describe('parseDue', () => {
    it('accepts every known due filter value', () => {
        expect(parseDue('not_due')).toBe('not_due')
        expect(parseDue('due_today')).toBe('due_today')
        expect(parseDue('overdue')).toBe('overdue')
        expect(parseDue('all')).toBe('all')
    })

    it('returns undefined for missing or unknown values', () => {
        expect(parseDue(null)).toBeUndefined()
        expect(parseDue('')).toBeUndefined()
        expect(parseDue('soon')).toBeUndefined()
    })
})
