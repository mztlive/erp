import { describe, expect, it } from 'vitest'

import type { SettlementsUrlState } from './url-state'
import {
    buildSettlementsSearchParams,
    parseSettlementsSearchParams,
} from './url-state'

function makeState(
    overrides: Partial<SettlementsUrlState> = {},
): SettlementsUrlState {
    return {
        view: 'pending',
        page: 1,
        section: 'overview',
        ...overrides,
    }
}

describe('parseSettlementsSearchParams', () => {
    it('falls back to defaults for empty params', () => {
        const parsed = parseSettlementsSearchParams(new URLSearchParams())
        expect(parsed).toEqual({
            view: 'pending',
            page: 1,
            section: 'overview',
        })
    })

    it('reads every supported param', () => {
        const parsed = parseSettlementsSearchParams(
            new URLSearchParams(
                'view=confirmed&supplier=sup1&periodFrom=2026-01-01&periodTo=2026-01-31' +
                    '&status=DRAFT&differenceType=AMOUNT&q=abc&page=3&preview=st9' +
                    '&id=st1&workItemId=w1&queueContextId=q1&from=W02' +
                    '&section=differences&returnTo=%2Fback&diff=d1',
            ),
        )
        expect(parsed).toEqual({
            view: 'confirmed',
            supplierId: 'sup1',
            periodFrom: '2026-01-01',
            periodTo: '2026-01-31',
            status: 'DRAFT',
            differenceType: 'AMOUNT',
            q: 'abc',
            page: 3,
            preview: 'st9',
            statementId: 'st1',
            workItemId: 'w1',
            queueContextId: 'q1',
            from: 'W02',
            section: 'differences',
            returnTo: '/back',
            diff: 'd1',
        })
    })

    it('normalizes invalid values back to defaults', () => {
        const parsed = parseSettlementsSearchParams(
            new URLSearchParams('view=bogus&page=abc&differenceType=UNKNOWN'),
        )
        expect(parsed).toEqual({
            view: 'pending',
            page: 1,
            section: 'overview',
        })
    })

    it('clamps non-positive page numbers to page 1', () => {
        const parsed = parseSettlementsSearchParams(
            new URLSearchParams('page=0'),
        )
        expect(parsed.page).toBe(1)

        const negative = parseSettlementsSearchParams(
            new URLSearchParams('page=-4'),
        )
        expect(negative.page).toBe(1)
    })

    it('keeps raw q on parse, trims on build, and resolves legacy aliases', () => {
        const parsed = parseSettlementsSearchParams(
            new URLSearchParams('q=%20%20abc%20%20&supplierId=sup1&period=2026-01'),
        )
        expect(parsed.q).toBe('  abc  ')
        expect(parsed.supplierId).toBe('sup1')
        expect(parsed.periodFrom).toBe('2026-01')

        const built = buildSettlementsSearchParams(makeState({ q: '  abc  ' }))
        expect(built).toBe('?q=abc')
    })
})

describe('buildSettlementsSearchParams', () => {
    it('builds an empty query string for the default state', () => {
        expect(buildSettlementsSearchParams(makeState())).toBe('')
    })

    it('writes only non-default fields under their url keys', () => {
        const qs = buildSettlementsSearchParams(
            makeState({
                view: 'confirmed',
                supplierId: 'sup1',
                q: 'abc',
                page: 2,
                status: 'DRAFT',
                differenceType: 'AMOUNT',
            }),
        )
        expect(qs).toBe(
            '?view=confirmed&supplier=sup1&status=DRAFT&differenceType=AMOUNT&q=abc&page=2',
        )
    })

    it('writes section only for a detail statement away from overview', () => {
        const withStatement = buildSettlementsSearchParams(
            makeState({ section: 'differences', statementId: 'st1' }),
        )
        expect(withStatement).toBe('?statementId=st1&section=differences')

        const withoutStatement = buildSettlementsSearchParams(
            makeState({ section: 'differences' }),
        )
        expect(withoutStatement).toBe('')
    })

    it('round-trips parse and build without losing meaningful fields', () => {
        const state = makeState({
            view: 'confirmed',
            supplierId: 'sup1',
            status: 'DRAFT',
            q: 'abc',
            page: 2,
            preview: 'st9',
            statementId: 'st1',
            section: 'differences',
            diff: 'd1',
        })
        const parsed = parseSettlementsSearchParams(
            new URLSearchParams(buildSettlementsSearchParams(state)),
        )
        expect(parsed).toEqual(state)
    })
})
