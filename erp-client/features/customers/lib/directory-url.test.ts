import { describe, it, expect } from 'vitest'

import {
    parsePage,
    SORT_COLUMN_TO_FIELD,
    writeDirectoryUrl,
} from '@/features/customers/lib/directory-url'

describe('parsePage', () => {
    it('parses positive integers', () => {
        expect(parsePage('3')).toBe(3)
        expect(parsePage('1')).toBe(1)
    })

    it('falls back to page 1 for missing, invalid or non-positive values', () => {
        expect(parsePage(null)).toBe(1)
        expect(parsePage('')).toBe(1)
        expect(parsePage('0')).toBe(1)
        expect(parsePage('-2')).toBe(1)
        expect(parsePage('abc')).toBe(1)
        expect(parsePage('1.5')).toBe(1)
    })
})

describe('writeDirectoryUrl', () => {
    const defaults = {
        scope: 'mine' as const,
        status: 'active' as const,
        q: '',
        sort: 'business',
        dir: 'desc' as const,
        page: 1,
    }

    it('writes a bare pathname for default values', () => {
        expect(writeDirectoryUrl('/sales/customers', defaults)).toBe(
            '/sales/customers',
        )
    })

    it('omits scope=status=q defaults and encodes the keyword', () => {
        expect(
            writeDirectoryUrl('/sales/customers', {
                ...defaults,
                q: '客户 甲',
            }),
        ).toBe('/sales/customers?q=%E5%AE%A2%E6%88%B7+%E7%94%B2')
    })

    it('writes non-default scope, status, sort, dir and page', () => {
        expect(
            writeDirectoryUrl('/sales/customers', {
                ...defaults,
                scope: 'all_authorized',
                status: 'all',
                sort: 'updated_at',
                dir: 'asc',
                page: 4,
            }),
        ).toBe(
            '/sales/customers?scope=all_authorized&status=all&sort=updated_at&dir=asc&page=4',
        )
    })

    it('does not emit the business sort column or page 1', () => {
        expect(
            writeDirectoryUrl('/sales/customers', {
                ...defaults,
                sort: 'business',
                page: 1,
            }),
        ).toBe('/sales/customers')
    })
})

describe('SORT_COLUMN_TO_FIELD', () => {
    it('maps the business column to the backend updated_at field', () => {
        expect(SORT_COLUMN_TO_FIELD.business).toBe('updated_at')
    })
})
