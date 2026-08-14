import { describe, it, expect } from 'vitest'

import { contractsUrlCodec } from '@/features/contracts/lib/contracts-url-state'

describe('contractsUrlCodec', () => {
    it('parses empty params to defaults', () => {
        expect(contractsUrlCodec.parse(new URLSearchParams())).toEqual({
            q: undefined,
            metric: 'all',
            page: 1,
            pageSize: 20,
            sort: undefined,
            dir: undefined,
            customerId: undefined,
        })
    })

    it('parses all fields from query params', () => {
        const state = contractsUrlCodec.parse(
            new URLSearchParams(
                'q=甲&metric=expiring_30d&page=3&pageSize=50&sort=contractNo&dir=desc&customerId=c1',
            ),
        )
        expect(state).toEqual({
            q: '甲',
            metric: 'expiring_30d',
            page: 3,
            pageSize: 50,
            sort: 'contractNo',
            dir: 'desc',
            customerId: 'c1',
        })
    })

    it('falls back to defaults for invalid enum and out-of-range numbers', () => {
        const state = contractsUrlCodec.parse(
            new URLSearchParams(
                'metric=bogus&dir=up&page=0&pageSize=99999&pageSize2=x',
            ),
        )
        expect(state.metric).toBe('all')
        expect(state.dir).toBeUndefined()
        expect(state.page).toBe(1)
        expect(state.pageSize).toBe(100)
    })

    it('reads the legacy search alias for q', () => {
        const state = contractsUrlCodec.parse(
            new URLSearchParams('search=客户甲'),
        )
        expect(state.q).toBe('客户甲')
    })

    it('builds a minimal URL skipping default values', () => {
        expect(
            contractsUrlCodec.build({
                q: undefined,
                metric: 'all',
                page: 1,
                pageSize: 20,
                sort: undefined,
                dir: undefined,
                customerId: undefined,
            }),
        ).toBe('')
    })

    it('builds a URL with only the non-default fields, in field order', () => {
        expect(
            contractsUrlCodec.build({
                q: '甲',
                metric: 'effective',
                page: 2,
                pageSize: 30,
                sort: 'contractNo',
                dir: 'desc',
                customerId: 'c1',
            }),
        ).toBe(
            '?q=%E7%94%B2&metric=effective&page=2&pageSize=30&sort=contractNo&dir=desc&customerId=c1',
        )
    })

    it('round-trips a parsed state', () => {
        const params = new URLSearchParams(
            'q=合同&metric=terminated&page=2&pageSize=50&sort=sales&dir=asc',
        )
        const state = contractsUrlCodec.parse(params)
        expect(contractsUrlCodec.buildParams(state).toString()).toBe(
            params.toString(),
        )
    })
})
