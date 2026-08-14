import { describe, it, expect } from 'vitest'

import { formatAsOf } from '@/features/contracts/lib/format-as-of'

describe('formatAsOf', () => {
    it('formats a valid ISO timestamp', () => {
        const output = formatAsOf('2026-06-15T08:30:00.000Z')
        expect(typeof output).toBe('string')
        expect(output.length).toBeGreaterThan(0)
        // zh-CN "6月15日" 式输出
        expect(output).toContain('月')
        expect(output).toContain('日')
    })

    it('returns the input unchanged for an invalid timestamp', () => {
        expect(formatAsOf('not-a-date')).toBe('not-a-date')
    })
})
