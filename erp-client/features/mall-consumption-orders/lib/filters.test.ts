import { describe, it, expect } from 'vitest'

import type { MallConsumptionOrderApplied } from './filters'
import {
    buildMallConsumptionFilterChips,
    EMPTY_MALL_CONSUMPTION_ORDER_FILTER_DRAFT,
    hasAppliedMallConsumptionFilters,
    hasStructuredMallConsumptionFilters,
    MALL_CONSUMPTION_FILTER_PARAM_KEYS,
    toMallConsumptionFilterDraft,
} from './filters'

const baseApplied: MallConsumptionOrderApplied = {
    q: '',
    mallId: 'all',
    attributionStatus: 'all',
    fulfillmentChain: 'all',
    paymentSource: 'all',
    costBasis: 'all',
    factTypes: [],
    supplierStatuses: [],
    dataSources: [],
    occurredFrom: '',
    occurredTo: '',
    metric: 'all',
}

describe('toMallConsumptionFilterDraft', () => {
    it('maps applied values into the draft shape with defaults', () => {
        const draft = toMallConsumptionFilterDraft(baseApplied)

        expect(draft).toEqual(EMPTY_MALL_CONSUMPTION_ORDER_FILTER_DRAFT)
    })

    it('maps "all" mall back to an empty string and copies arrays', () => {
        const draft = toMallConsumptionFilterDraft({
            ...baseApplied,
            mallId: 'm1',
            attributionStatus: 'PENDING',
            factTypes: ['PAYMENT_SUCCEEDED', 'REFUND_SUCCEEDED'],
            occurredFrom: '2026-08-01',
            occurredTo: '2026-08-07',
        })

        expect(draft.mallId).toBe('m1')
        expect(draft.attributionStatus).toBe('PENDING')
        expect(draft.factTypes).toEqual([
            'PAYMENT_SUCCEEDED',
            'REFUND_SUCCEEDED',
        ])
        expect(draft.occurredFrom).toBe('2026-08-01')
        expect(draft.occurredTo).toBe('2026-08-07')
        expect(draft.factTypes).not.toBe(baseApplied.factTypes)
    })
})

describe('hasStructuredMallConsumptionFilters', () => {
    it('is false when only q, metric or the period is applied', () => {
        expect(
            hasStructuredMallConsumptionFilters({
                ...baseApplied,
                q: 'x',
            }),
        ).toBe(false)
        expect(
            hasStructuredMallConsumptionFilters({
                ...baseApplied,
                metric: 'paid',
            }),
        ).toBe(false)
        expect(
            hasStructuredMallConsumptionFilters({
                ...baseApplied,
                occurredFrom: '2026-08-01',
                occurredTo: '2026-08-07',
            }),
        ).toBe(false)
    })

    it('is true for any panel field', () => {
        expect(
            hasStructuredMallConsumptionFilters({
                ...baseApplied,
                mallId: 'm1',
            }),
        ).toBe(true)
        expect(
            hasStructuredMallConsumptionFilters({
                ...baseApplied,
                factTypes: ['ORDER_CANCELED'],
            }),
        ).toBe(true)
        expect(
            hasStructuredMallConsumptionFilters({
                ...baseApplied,
                costBasis: 'NONE',
            }),
        ).toBe(true)
    })
})

describe('hasAppliedMallConsumptionFilters', () => {
    it('covers q and metric in addition to structured fields', () => {
        expect(hasAppliedMallConsumptionFilters(baseApplied)).toBe(false)
        expect(
            hasAppliedMallConsumptionFilters({ ...baseApplied, q: '  ' }),
        ).toBe(false)
        expect(
            hasAppliedMallConsumptionFilters({ ...baseApplied, q: 'a' }),
        ).toBe(true)
        expect(
            hasAppliedMallConsumptionFilters({ ...baseApplied, metric: 'paid' }),
        ).toBe(true)
    })
})

describe('buildMallConsumptionFilterChips', () => {
    const malls = [{ id: 'm1', name: '华东商城' }]

    it('returns no chips for the empty state', () => {
        expect(buildMallConsumptionFilterChips(baseApplied, [])).toEqual([])
    })

    it('shows every applied filter as a removable chip with business labels', () => {
        const chips = buildMallConsumptionFilterChips(
            {
                ...baseApplied,
                q: 'SO-1',
                mallId: 'm1',
                attributionStatus: 'DIFFERENCE',
                fulfillmentChain: 'ERP_AUTOMATED',
                paymentSource: 'MIXED',
                costBasis: 'NONE',
                factTypes: ['PAYMENT_SUCCEEDED', 'REFUND_SUCCEEDED'],
                supplierStatuses: ['SHIPPED', 'EXCEPTION'],
                dataSources: ['REALTIME', 'BACKFILL'],
                metric: 'cost_none',
            },
            malls,
        )

        expect(chips.map((chip) => chip.key)).toEqual([
            'q',
            'mall',
            'attributionStatus',
            'fulfillmentChain',
            'paymentSource',
            'costBasis',
            'factTypes',
            'supplierStatuses',
            'dataSources',
            'metric',
        ])
        expect(chips.map((chip) => chip.label)).toEqual([
            '搜索：SO-1',
            '来源商城：华东商城',
            '归集：差异',
            '履约链：ERP 自动履约',
            '支付方式：组合',
            '成本口径：无成本',
            '事实类型：支付成功、商城退款成功',
            '供应商状态：已发货、异常',
            '数据来源：实时、历史回填',
            '指标：成本未覆盖',
        ])
    })

    it('falls back to the raw mall id when the mall list has not loaded', () => {
        const chips = buildMallConsumptionFilterChips(
            { ...baseApplied, mallId: 'm9' },
            [],
        )

        expect(chips).toEqual([{ key: 'mall', label: '来源商城：m9' }])
    })

    it('never exposes raw enum values in chip labels', () => {
        const chips = buildMallConsumptionFilterChips(
            {
                ...baseApplied,
                attributionStatus: 'PENDING',
                fulfillmentChain: 'LEGACY_MANUAL',
                paymentSource: 'CARD',
                costBasis: 'ACTUAL',
                factTypes: ['ORDER_COMPLETED'],
                supplierStatuses: ['RECEIVED'],
                dataSources: ['REALTIME'],
            },
            [],
        )

        const labels = chips.map((chip) => chip.label).join(' ')
        expect(labels).not.toMatch(/PENDING|LEGACY_MANUAL|CARD|ACTUAL|ORDER_COMPLETED|RECEIVED|REALTIME/)
        expect(labels).toContain('归集：待归集')
        expect(labels).toContain('履约链：原人工履约')
        expect(labels).toContain('支付方式：卡券')
        expect(labels).toContain('成本口径：实际成本')
        expect(labels).toContain('事实类型：商城订单已完成')
        expect(labels).toContain('供应商状态：已接收')
        expect(labels).toContain('数据来源：实时')
    })

    it('declares the canonical filter param list used by apply and clear', () => {
        expect(MALL_CONSUMPTION_FILTER_PARAM_KEYS).toEqual([
            'q',
            'mall',
            'attributionStatus',
            'fulfillmentChain',
            'paymentSource',
            'costBasis',
            'factType',
            'supplierStatus',
            'dataSource',
            'metric',
        ])
    })
})
