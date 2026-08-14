import { describe, it, expect } from 'vitest'

import type {
    MallConsumptionOrderRow,
    MallConsumptionOrderView,
} from '@/features/mall-consumption-orders/types'
import {
    costBasisLabel,
    factSummaryLabel,
    paymentCompositionLabel,
    previewDataSourceLabel,
    supplierSummaryLabel,
} from './labels'

const baseRow: MallConsumptionOrderRow = {
    mallOrderId: 'mo-1',
    mallId: 'm1',
    mallName: '商城 A',
    externalOrderNo: 'SO-1',
    customerLabel: '客户甲',
    paidAt: '2026-08-01T00:00:00.000Z',
    paidAmount: '100.00',
    paymentComposition: { cardAmount: '0.00', wechatAmount: '0.00', sourceCount: 2 },
    factSummary: [],
    fulfillmentChain: 'ERP_AUTOMATED',
    supplierOrderSummary: { total: 0, statuses: [], hasException: false },
    attributionStatus: 'ATTRIBUTED',
    costBasisBreakdown: [],
    dataSource: 'REALTIME',
    allowedActions: [],
    actionBlockers: [],
    costBasisPolicyState: 'CONFIGURED',
}

describe('paymentCompositionLabel', () => {
    it('labels a mixed composition', () => {
        expect(
            paymentCompositionLabel({
                ...baseRow,
                paymentComposition: {
                    cardAmount: '60.00',
                    wechatAmount: '40.00',
                    sourceCount: 2,
                },
            }),
        ).toBe('组合 · 卡券 ¥60.00 / 微信 ¥40.00')
    })

    it('labels card-only and wechat-only compositions', () => {
        expect(
            paymentCompositionLabel({
                ...baseRow,
                paymentComposition: {
                    cardAmount: '100.00',
                    wechatAmount: '0.00',
                    sourceCount: 1,
                },
            }),
        ).toBe('卡券 ¥100.00')
        expect(
            paymentCompositionLabel({
                ...baseRow,
                paymentComposition: {
                    cardAmount: '0.00',
                    wechatAmount: '100.00',
                    sourceCount: 1,
                },
            }),
        ).toBe('微信 ¥100.00')
    })

    it('falls back to the source count when no amount is present', () => {
        expect(paymentCompositionLabel(baseRow)).toBe('2 来源')
    })
})

describe('factSummaryLabel', () => {
    it('joins fact labels and repeats counts', () => {
        expect(
            factSummaryLabel({
                ...baseRow,
                factSummary: [
                    {
                        factType: 'PAYMENT_SUCCEEDED',
                        latestOccurredAt: '',
                        count: 2,
                    },
                    {
                        factType: 'REFUND_SUCCEEDED',
                        latestOccurredAt: '',
                        count: 1,
                    },
                ],
            }),
        ).toBe('支付成功×2 · 商城退款成功')
    })

    it('returns an empty string without facts', () => {
        expect(factSummaryLabel(baseRow)).toBe('')
    })
})

describe('costBasisLabel', () => {
    it('joins cost basis labels with repeats', () => {
        expect(
            costBasisLabel({
                ...baseRow,
                costBasisBreakdown: [
                    { basis: 'ACTUAL', lineCount: 2, costAmount: '10.00' },
                    { basis: 'STANDARD', lineCount: 1 },
                ],
            }),
        ).toBe('实际成本×2 / 标准成本')
    })

    it('returns an empty string without breakdown', () => {
        expect(costBasisLabel(baseRow)).toBe('')
    })
})

describe('supplierSummaryLabel', () => {
    it('labels the legacy manual chain without sub-orders', () => {
        expect(
            supplierSummaryLabel({
                ...baseRow,
                fulfillmentChain: 'LEGACY_MANUAL',
            }),
        ).toBe('原人工 · 无子订单')
    })

    it('labels an automated chain without sub-orders', () => {
        expect(supplierSummaryLabel(baseRow)).toBe('尚未生成子订单')
    })

    it('joins sub-order statuses and flags exceptions', () => {
        expect(
            supplierSummaryLabel({
                ...baseRow,
                supplierOrderSummary: {
                    total: 2,
                    statuses: ['SHIPPED', 'COMPLETED'],
                    hasException: true,
                },
            }),
        ).toBe('2 单 · 已发货/已完成 · 异常')
    })

    it('keeps unknown statuses as-is', () => {
        expect(
            supplierSummaryLabel({
                ...baseRow,
                supplierOrderSummary: {
                    total: 1,
                    statuses: ['MYSTERY'],
                    hasException: false,
                },
            }),
        ).toBe('1 单 · MYSTERY')
    })
})

describe('previewDataSourceLabel', () => {
    const viewWithFacts = (
        sources: Array<"REALTIME" | "BACKFILL">,
    ): MallConsumptionOrderView =>
        ({
            facts: sources.map((dataSource) => ({ dataSource })),
        }) as MallConsumptionOrderView

    it('returns a dash without facts', () => {
        expect(previewDataSourceLabel(viewWithFacts([]))).toBe('—')
    })

    it('labels a single source kind', () => {
        expect(previewDataSourceLabel(viewWithFacts(['REALTIME']))).toBe('实时')
        expect(previewDataSourceLabel(viewWithFacts(['BACKFILL']))).toBe(
            '历史回填',
        )
    })

    it('labels mixed sources', () => {
        expect(
            previewDataSourceLabel(
                viewWithFacts(['REALTIME', 'BACKFILL']),
            ),
        ).toBe('混合')
    })
})
