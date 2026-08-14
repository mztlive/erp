import { describe, it, expect } from 'vitest'

import { formatMoney, moneyStrSafe, shortHash } from './presentation'

describe('shortHash', () => {
    it('returns short values unchanged', () => {
        expect(shortHash('abc')).toBe('abc')
        expect(shortHash('12345678901234567890')).toBe(
            '12345678901234567890',
        )
    })

    it('truncates long values with an ellipsis', () => {
        const long = 'a'.repeat(40)
        expect(shortHash(long)).toBe(`${'a'.repeat(12)}…${'a'.repeat(6)}`)
    })
})

describe('formatMoney', () => {
    it('formats CNY with two decimals', () => {
        expect(formatMoney('1130')).toBe('¥1,130.00')
        expect(formatMoney('0')).toBe('¥0.00')
        expect(formatMoney('12.5')).toBe('¥12.50')
    })

    it('treats unparseable values as zero', () => {
        expect(formatMoney('abc')).toBe('¥0.00')
        expect(formatMoney('')).toBe('¥0.00')
    })
})

describe('moneyStrSafe', () => {
    it('fixes values to two decimals', () => {
        expect(moneyStrSafe(100)).toBe('100.00')
        expect(moneyStrSafe(113 / 1.13)).toBe('100.00')
    })

    it('returns 0.00 for non-finite values', () => {
        expect(moneyStrSafe(Number.NaN)).toBe('0.00')
        expect(moneyStrSafe(Number.POSITIVE_INFINITY)).toBe('0.00')
    })
})
