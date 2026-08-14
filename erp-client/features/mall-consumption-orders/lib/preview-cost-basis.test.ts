import { describe, it, expect } from 'vitest'

import type { MallConsumptionOrderView } from '@/features/mall-consumption-orders/types'
import { derivePrimaryCostBasis } from './preview-cost-basis'

const viewWithBases = (
    bases: Array<"ACTUAL" | "STANDARD" | "NONE">,
): MallConsumptionOrderView =>
    ({
        consumptionEntries: bases.map((basis) => ({
            currentCostAssessment: { costBasis: basis },
        })),
    }) as MallConsumptionOrderView

describe('derivePrimaryCostBasis', () => {
    it('returns NONE for an empty entry list', () => {
        expect(derivePrimaryCostBasis(viewWithBases([]))).toBe('NONE')
    })

    it('returns NONE when every entry has no cost', () => {
        expect(derivePrimaryCostBasis(viewWithBases(['NONE', 'NONE']))).toBe(
            'NONE',
        )
    })

    it('prefers ACTUAL over other bases', () => {
        expect(
            derivePrimaryCostBasis(
                viewWithBases(['STANDARD', 'ACTUAL', 'NONE']),
            ),
        ).toBe('ACTUAL')
    })

    it('falls back to STANDARD when no ACTUAL entry exists', () => {
        expect(
            derivePrimaryCostBasis(viewWithBases(['NONE', 'STANDARD'])),
        ).toBe('STANDARD')
    })

    it('returns NONE when only non-cost entries exist but at least one differs', () => {
        // 有成本评估但全部 NONE 之外的组合不会出现：这里验证单一 NONE 不满足“全部 NONE”
        expect(derivePrimaryCostBasis(viewWithBases(['NONE']))).toBe('NONE')
    })
})
