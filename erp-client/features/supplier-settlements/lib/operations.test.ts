import { describe, expect, it } from 'vitest'

import type { FormalOutcome } from '@/features/supplier-settlements/types'
import { blockerOf, newKey, outcomeToResult } from './operations'

function makeOutcome(
    overrides: Partial<FormalOutcome> = {},
): FormalOutcome {
    return {
        status: 'succeeded',
        title: '结算草稿已创建',
        message: '已创建',
        ...overrides,
    }
}

describe('outcomeToResult', () => {
    it('maps a succeeded outcome with a payable link', () => {
        const result = outcomeToResult(
            makeOutcome({
                reference: 'ST-1',
                payableNo: 'PY-9',
                facts: [{ label: '结算单号', value: 'ST-1' }],
            }),
        )
        expect(result?.status).toBe('succeeded')
        expect(result?.title).toBe('结算草稿已创建')
        expect(result?.reference).toBe('ST-1')
        expect(result?.facts).toEqual([{ label: '结算单号', value: 'ST-1' }])
        expect(result?.w12Href).toBe(
            '/finance/supplier-accounts?view=payable&sourceType=SUPPLIER_SETTLEMENT&q=PY-9',
        )
    })

    it('maps a succeeded outcome without a payable to no w12 link', () => {
        const result = outcomeToResult(makeOutcome({ reference: 'ST-1' }))
        expect(result?.status).toBe('succeeded')
        expect(result?.w12Href).toBeUndefined()
    })

    it('url-encodes the payable number in the w12 link', () => {
        const result = outcomeToResult(makeOutcome({ payableNo: 'PY 9/8' }))
        expect(result?.w12Href).toBe(
            '/finance/supplier-accounts?view=payable&sourceType=SUPPLIER_SETTLEMENT&q=PY%209%2F8',
        )
    })

    it('maps unknown outcomes without reference or facts', () => {
        const result = outcomeToResult(
            makeOutcome({ status: 'unknown', title: '处理结果待确认' }),
        )
        expect(result).toEqual({
            status: 'unknown',
            title: '处理结果待确认',
            description: '已创建',
        })
    })

    it('maps rejected outcomes and keeps reference and facts', () => {
        const result = outcomeToResult(
            makeOutcome({
                status: 'rejected',
                title: '结算已驳回',
                reference: 'ST-1',
            }),
        )
        expect(result).toEqual({
            status: 'rejected',
            title: '结算已驳回',
            description: '已创建',
            reference: 'ST-1',
            facts: undefined,
        })
    })

    it('maps blocked and failed outcomes to their result states', () => {
        expect(outcomeToResult(makeOutcome({ status: 'blocked' }))?.status).toBe(
            'blocked',
        )
        expect(outcomeToResult(makeOutcome({ status: 'failed' }))?.status).toBe(
            'failed',
        )
    })
})

describe('newKey', () => {
    it('prefixes generated keys and keeps them unique', () => {
        const a = newKey('req')
        const b = newKey('req')
        expect(a.startsWith('req_')).toBe(true)
        expect(b.startsWith('req_')).toBe(true)
        expect(a).not.toBe(b)
    })
})

describe('blockerOf', () => {
    const blockers = [
        { action: 'SUBMIT_REVIEW', code: 'X', message: '不可提交' },
        { action: 'REVIEW_DECISION', code: 'Y', message: '不可复核' },
    ]

    it('finds the blocker for a given action', () => {
        expect(blockerOf(blockers, 'REVIEW_DECISION')).toEqual({
            action: 'REVIEW_DECISION',
            code: 'Y',
            message: '不可复核',
        })
    })

    it('returns undefined when no blocker matches', () => {
        expect(blockerOf(blockers, 'RESOLVE_DIFFERENCE')).toBeUndefined()
        expect(blockerOf([], 'SUBMIT_REVIEW')).toBeUndefined()
    })
})
