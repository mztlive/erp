import type {
    CardBusinessAnalyticsView,
    CardBusinessRow,
} from '../types'

/** 测试用完整视图桩：字段齐全，可按需覆盖。 */
export function makeStubView(
    overrides?: Partial<CardBusinessAnalyticsView>,
): CardBusinessAnalyticsView {
    return {
        scope: {
            timezone: 'Asia/Shanghai',
            currency: 'CNY',
            filterDigest: 'digest',
            permissionVersion: 'v1',
            scopeLabel: '全部',
        },
        period: {
            from: '2026-08-01',
            to: '2026-08-07',
            dateBasis: 'consumption',
            dateBasisLabel: '消费发生日',
        },
        freshness: {
            projectionUpdatedAt: '2026-08-07T10:00:00Z',
            consumedOutboxWatermark: '2026-08-07T09:59:00Z',
            sourceFactWatermark: '2026-08-07T09:58:00Z',
            balanceSnapshotAt: '2026-08-07T09:57:00Z',
            lagSeconds: 30,
            maxLagSeconds: 60,
            slaState: 'WITHIN_SLA',
            state: 'fresh',
        },
        coverage: {
            coveredConsumptionGross: '800.00',
            totalConsumptionGross: '1000.00',
            rate: '80%',
            ratePercent: 80,
            threshold: '70%',
            status: 'acceptable',
            byBasis: [
                {
                    basis: 'ACTUAL',
                    consumptionGross: '600.00',
                    costNet: '400.00',
                    share: '60%',
                    shareLabel: '60%',
                },
                {
                    basis: 'STANDARD',
                    consumptionGross: '300.00',
                    costNet: '200.00',
                    share: '30%',
                    shareLabel: '30%',
                },
                {
                    basis: 'NONE',
                    consumptionGross: '100.00',
                    share: '10%',
                    shareLabel: '10%',
                },
            ],
            dominantBasis: 'ACTUAL',
            notice: '成本覆盖良好。',
            profitReferenceOnly: false,
        },
        metrics: [
            {
                key: 'salesGross',
                label: '销售',
                value: '5000.00',
                taxBasis: 'GROSS',
                currency: 'CNY',
                valueState: 'available',
            },
            {
                key: 'currentContributionNet',
                label: '当前经营贡献',
                value: '800.00',
                taxBasis: 'NET',
                currency: 'CNY',
                valueState: 'available',
            },
        ],
        scopeFullyExpired: true,
        finalProfitNet: '800.00',
        trends: {
            consumption: [
                {
                    period: '2026-W31',
                    salesGross: '1000.00',
                    consumptionGross: '800.00',
                    refundGross: '50.00',
                    balanceGross: '200.00',
                },
            ],
            contribution: [
                {
                    period: '2026-W31',
                    marginNet: '300.00',
                    contributionNet: '400.00',
                    coverageRate: '80%',
                    coveragePercent: 80,
                },
            ],
        },
        breakdowns: {
            byCategory: [
                { id: 'c1', label: '餐饮券', consumptionGross: '600.00', share: '60%' },
            ],
            byCustomer: [
                { id: 'cu1', label: '示例客户', consumptionGross: '400.00', share: '40%' },
            ],
        },
        rows: {
            items: [makeStubRow()],
            total: 1,
        },
        filterSummary: '期间 2026-08-01 ~ 2026-08-07',
        wechatExcludedNote: '微信支付消费不进入企业卡券指标。',
        wechatExcluded: { consumptionGross: '10.00', costNet: '5.00' },
        fieldPermissions: { canViewCost: true, canViewProfit: true, canExport: true },
        governanceLinks: {
            noneCoverageHref: '/integration-errors?filter=none-coverage',
            backfillHref: '/history-backfill',
            integrationErrorsHref: '/integration-errors',
        },
        ...overrides,
    }
}

/** 测试用明细行桩。 */
export function makeStubRow(
    overrides?: Partial<CardBusinessRow>,
): CardBusinessRow {
    return {
        rowId: 'row-1',
        customerId: 'cu1',
        customerLabel: '示例客户',
        salesOrderId: 'so1',
        salesOrderNo: 'SO-2026-001',
        voucherCategoryLabel: '餐饮券',
        cardInstanceRef: 'ref-abc',
        consumptionGross: '100.00',
        refundGross: '0.00',
        costBasis: 'ACTUAL',
        costNet: '60.00',
        coverageStatus: 'covered',
        unconsumedBalanceGross: '50.00',
        unfulfilledBalanceGross: '30.00',
        riskLabel: '低',
        consumptionOrderHref: '/mall/orders/m1',
        supplierOrderHref: '/suppliers/orders/s1',
        ...overrides,
    }
}
