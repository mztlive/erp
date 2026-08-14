import { describe, it, expect } from 'vitest'

import type { MallOrderFactView } from '@/features/mall-consumption-orders/types'

import {
    computeCostBasisPrimary,
    computeCostCoverage,
    resolveSelectedFactId,
    sortFactsByOccurredAt,
} from './consumption-order-center-derivations'

function entry(basis: 'ACTUAL' | 'STANDARD' | 'NONE') {
    return { currentCostAssessment: { costBasis: basis } }
}

function fact(
    factId: string,
    occurredAt: string,
    factType: MallOrderFactView['factType'] = 'PAYMENT_SUCCEEDED',
): MallOrderFactView {
    return {
        factId,
        factType,
        businessFactKeySummary: '',
        externalOrderVersion: '1',
        occurredAt,
        receivedAt: occurredAt,
        dataSource: 'REALTIME',
        processingStatus: 'SAVED',
        resultDetails: {},
    }
}

describe('computeCostBasisPrimary', () => {
    it('returns NONE for an empty entry list', () => {
        expect(computeCostBasisPrimary([])).toBe('NONE')
    })

    it('returns NONE when every entry has no cost', () => {
        expect(computeCostBasisPrimary([entry('NONE'), entry('NONE')])).toBe(
            'NONE',
        )
    })

    it('returns ACTUAL when any entry carries actual cost', () => {
        expect(
            computeCostBasisPrimary([entry('NONE'), entry('ACTUAL')]),
        ).toBe('ACTUAL')
    })

    it('returns STANDARD when only standard costs exist', () => {
        expect(
            computeCostBasisPrimary([entry('STANDARD'), entry('NONE')]),
        ).toBe('STANDARD')
    })

    it('prefers ACTUAL over STANDARD', () => {
        expect(
            computeCostBasisPrimary([entry('STANDARD'), entry('ACTUAL')]),
        ).toBe('ACTUAL')
    })
})

describe('computeCostCoverage', () => {
    it('reports none for an empty entry list', () => {
        expect(computeCostCoverage([])).toEqual({
            total: 0,
            coveredCount: 0,
            percent: 0,
            state: 'none',
        })
    })

    it('reports complete with 100% when all entries are covered', () => {
        expect(computeCostCoverage([entry('ACTUAL'), entry('STANDARD')])).toEqual(
            {
                total: 2,
                coveredCount: 2,
                percent: 100,
                state: 'complete',
            },
        )
    })

    it('reports partial with rounded percent for mixed coverage', () => {
        expect(
            computeCostCoverage([entry('ACTUAL'), entry('NONE'), entry('NONE')]),
        ).toEqual({
            total: 3,
            coveredCount: 1,
            percent: 33,
            state: 'partial',
        })
    })

    it('reports none when no entry has cost', () => {
        expect(computeCostCoverage([entry('NONE')])).toEqual({
            total: 1,
            coveredCount: 0,
            percent: 0,
            state: 'none',
        })
    })
})

describe('sortFactsByOccurredAt', () => {
    it('sorts facts ascending by occurrence time', () => {
        const facts = [
            fact('f-2', '2026-08-02T10:00:00Z', 'ORDER_COMPLETED'),
            fact('f-1', '2026-08-01T10:00:00Z'),
            fact('f-3', '2026-08-03T10:00:00Z', 'REFUND_SUCCEEDED'),
        ]
        expect(sortFactsByOccurredAt(facts).map((f) => f.factId)).toEqual([
            'f-1',
            'f-2',
            'f-3',
        ])
    })

    it('does not mutate the input array', () => {
        const facts = [
            fact('f-2', '2026-08-02T10:00:00Z'),
            fact('f-1', '2026-08-01T10:00:00Z'),
        ]
        sortFactsByOccurredAt(facts)
        expect(facts.map((f) => f.factId)).toEqual(['f-2', 'f-1'])
    })
})

describe('resolveSelectedFactId', () => {
    it('prefers the explicit fact id', () => {
        expect(
            resolveSelectedFactId('f-9', [
                fact('f-1', '2026-08-01T10:00:00Z'),
            ]),
        ).toBe('f-9')
    })

    it('prefers the payment-succeeded fact when nothing is explicit', () => {
        expect(
            resolveSelectedFactId(undefined, [
                fact('f-1', '2026-08-01T10:00:00Z', 'ORDER_COMPLETED'),
                fact('f-2', '2026-08-02T10:00:00Z'),
            ]),
        ).toBe('f-2')
    })

    it('falls back to the first fact without a payment record', () => {
        expect(
            resolveSelectedFactId(undefined, [
                fact('f-1', '2026-08-01T10:00:00Z', 'ORDER_COMPLETED'),
                fact('f-3', '2026-08-03T10:00:00Z', 'REFUND_SUCCEEDED'),
            ]),
        ).toBe('f-1')
    })

    it('returns undefined for an empty fact list', () => {
        expect(resolveSelectedFactId(undefined, [])).toBeUndefined()
    })
})
