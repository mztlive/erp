import { describe, it, expect } from 'vitest'

import {
    DATA_SOURCES,
    FACT_TYPES,
    SUPPLIER_STATUSES,
    parseMetric,
    parseMultiValue,
    parseSection,
} from './url-state'

describe('parseMetric', () => {
    it('accepts every known metric key', () => {
        expect(parseMetric('paid')).toBe('paid')
        expect(parseMetric('pending_attr')).toBe('pending_attr')
        expect(parseMetric('fact_diff')).toBe('fact_diff')
        expect(parseMetric('auto_exception')).toBe('auto_exception')
        expect(parseMetric('cost_none')).toBe('cost_none')
    })

    it('falls back to "all" for missing or unknown values', () => {
        expect(parseMetric(null)).toBe('all')
        expect(parseMetric('')).toBe('all')
        expect(parseMetric('bogus')).toBe('all')
    })
})

describe('parseMultiValue', () => {
    it('returns an empty array for missing values', () => {
        expect(parseMultiValue(null, ['A', 'B'])).toEqual([])
        expect(parseMultiValue('', ['A', 'B'])).toEqual([])
    })

    it('splits and trims comma-separated values', () => {
        expect(parseMultiValue('A, B ,,C', ['A', 'B', 'C'])).toEqual([
            'A',
            'B',
            'C',
        ])
    })

    it('drops values outside the whitelist', () => {
        expect(parseMultiValue('A,NOPE,b', ['A', 'B'])).toEqual(['A'])
    })

    it('keeps duplicates in order (implementation does not dedupe)', () => {
        expect(parseMultiValue('A,A,B,A', ['A', 'B'])).toEqual([
            'A',
            'A',
            'B',
            'A',
        ])
    })
})

describe('whitelist constants', () => {
    it('exposes the whitelists used by the list page', () => {
        expect(FACT_TYPES).toContain('PAYMENT_SUCCEEDED')
        expect(SUPPLIER_STATUSES).toContain('SHIPPED')
        expect(DATA_SOURCES).toEqual(['REALTIME', 'BACKFILL'])
    })
})

describe('parseSection', () => {
    it('returns the requested section when known', () => {
        expect(parseSection('facts')).toBe('facts')
        expect(parseSection('supplier')).toBe('supplier')
    })

    it('falls back to overview for missing or unknown sections', () => {
        expect(parseSection(null)).toBe('overview')
        expect(parseSection('')).toBe('overview')
        expect(parseSection('bogus')).toBe('overview')
    })
})
