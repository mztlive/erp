import { describe, it, expect } from 'vitest'

import type { MallConsumptionOrderView } from '@/features/mall-consumption-orders/types'
import { customerLabelFor } from './customer-title'

const viewWith = (
    customerLabel: string,
    permission: "full" | "masked" | "hidden",
): MallConsumptionOrderView =>
    ({
        customer: { customerLabel },
        fieldPermissions: { customer: permission },
    }) as unknown as MallConsumptionOrderView

describe('customerLabelFor', () => {
    it('shows the customer label with full permission', () => {
        expect(customerLabelFor(viewWith('客户甲', 'full'))).toBe('客户甲')
    })

    it('replaces the customer label when masked', () => {
        expect(customerLabelFor(viewWith('客户甲', 'masked'))).toBe(
            '客户（已打码）',
        )
    })
})
