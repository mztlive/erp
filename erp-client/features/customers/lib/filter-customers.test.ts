import { describe, it, expect } from 'vitest'

import {
    parseCustomerScope,
    SCOPE_LABELS,
    SCOPE_ORDER,
} from '@/features/customers/lib/filter-customers'

describe('parseCustomerScope', () => {
    it('accepts the three directory scope values', () => {
        expect(parseCustomerScope('mine')).toBe('mine')
        expect(parseCustomerScope('collaborating')).toBe('collaborating')
        expect(parseCustomerScope('all_authorized')).toBe('all_authorized')
    })

    it('falls back to mine for unknown or missing values', () => {
        expect(parseCustomerScope('assigned')).toBe('mine')
        expect(parseCustomerScope('nonsense')).toBe('mine')
        expect(parseCustomerScope(null)).toBe('mine')
        expect(parseCustomerScope(undefined)).toBe('mine')
        expect(parseCustomerScope('')).toBe('mine')
    })
})

describe('scope tables', () => {
    it('labels every scope in order', () => {
        expect(SCOPE_ORDER).toEqual(['mine', 'collaborating', 'all_authorized'])
        expect(SCOPE_LABELS).toEqual({
            mine: '我的客户',
            collaborating: '协作客户',
            assigned: '我参与的客户',
            all_authorized: '全部有权客户',
        })
    })
})
