import type {
    CostEntryDetail,
    ProfitLossExportJob,
    ProfitLossPeriodBasisConfig,
    ProfitLossQuery,
    ProfitLossRow,
    ProfitLossView,
} from "@/features/actual-profit-loss/types"

export const fakeRow: ProfitLossRow = {
    rowId: "row-1",
    objectType: "sales_order",
    objectId: "so-1",
    identityLabel: "SO-2026-0001",
    customerId: "c-1",
    customerLabel: "示例客户",
    benefitScenarios: ["常规福利"],
    fulfillmentModes: ["电子履约"],
    netSalesRevenue: "1000.00",
    actualProcurementCostNet: "400.00",
    actualFulfillmentCostNet: "100.00",
    reductionsNet: "0.00",
    actualProfitLossNet: "500.00",
    marginRate: "50.0%",
    coverageState: "COVERED",
    coverageBlockers: [],
    latestCostOccurredAt: "2026-08-10T02:00:00.000Z",
    allowedDrilldowns: ["cost_entry"],
    costEntryIds: ["ce-1", "ce-2"],
}

export function makeRow(overrides: Partial<ProfitLossRow> = {}): ProfitLossRow {
    return { ...fakeRow, ...overrides }
}

export function makeView(overrides: Partial<ProfitLossView> = {}): ProfitLossView {
    return {
        scope: {
            id: "org-hq-finance",
            label: "总部财务",
            permissionVersion: "v1",
        },
        period: {
            from: "2026-08-01",
            to: "2026-08-14",
            basis: "sales_revenue_recognition_date",
            basisLabel: "销售收入确认日",
            timezone: "Asia/Shanghai",
        },
        businessType: "GOODS_SERVICE",
        amountBasis: "NET",
        amountBasisLabel: "不含税",
        businessTypeLabel: "非卡券",
        formulaVersion: "v2",
        formulaText: "实际经营盈亏（不含税）公式",
        freshness: {
            projectedAt: "2026-08-14T08:00:00.000Z",
            sourceWatermark: "2026-08-14T07:00:00.000Z",
            state: "fresh",
        },
        coverage: {
            coveredNetRevenue: "1000.00",
            uncoveredNetRevenue: "0.00",
            coverageRate: "100%",
            reliability: "reliable",
            coverageState: "complete",
        },
        totals: {
            netSalesRevenue: "1000.00",
            actualProcurementCostNet: "400.00",
            actualFulfillmentCostNet: "100.00",
            reductionsNet: "0.00",
            actualProfitLossNet: "500.00",
            marginRate: "50.0%",
        },
        fieldPermissions: {
            canViewRevenue: true,
            canViewCost: true,
            canViewProfit: true,
            canExport: true,
        },
        trend: [],
        costComposition: [],
        stageReference: [],
        rows: {
            dimension: "sales_order",
            items: [fakeRow],
            total: 1,
        },
        filterSummary: "范围：全部",
        excludedNote: "卡券与消费成本不在本页",
        ...overrides,
    }
}

export const allowedPeriodBases: ProfitLossPeriodBasisConfig["allowedPeriodBases"] =
    [
        {
            code: "sales_revenue_recognition_date",
            label: "销售收入确认日",
            explanation: "按销售收入确认日归属",
        },
        {
            code: "cost_occurred_date",
            label: "成本发生日",
            explanation: "按成本发生日归属",
        },
    ]

export const configuredBasis: ProfitLossPeriodBasisConfig = {
    configuredPeriodBasis: "sales_revenue_recognition_date",
    allowedPeriodBases,
    configurationVersion: "v3",
}

export const unconfiguredBasis: ProfitLossPeriodBasisConfig = {
    allowedPeriodBases,
    configurationVersion: "v3",
}

export const fakeCostEntries: CostEntryDetail[] = [
    {
        costEntryId: "ce-1",
        costType: "printing",
        costTypeLabel: "印刷",
        stage: "ACTUAL",
        stageLabel: "实际",
        costScope: "NON_VOUCHER_FULFILLMENT",
        costScopeLabel: "非卡券履约",
        supplierId: "s-1",
        supplierName: "示例供应商",
        amountGross: "113.00",
        taxRate: "13%",
        taxAmount: "13.00",
        amountNet: "100.00",
        occurredAt: "2026-08-10T02:00:00.000Z",
        sourceType: "cost_entry",
        sourceTypeLabel: "成本记录",
        sourceDocumentId: "doc-1",
        sourceDocumentNo: "doc-1",
        sourceVersion: "v1",
        salesOrderId: "so-1",
        salesOrderNo: "SO-2026-0001",
    },
]

export const fakeExportJob: ProfitLossExportJob = {
    jobId: "job-1",
    status: "queued",
    total: 1,
    completed: 0,
    createdAt: "2026-08-14T08:00:00.000Z",
    watermark: {
        periodFrom: "2026-08-01",
        periodTo: "2026-08-14",
        periodBasis: "sales_revenue_recognition_date",
        formulaVersion: "v2",
        coverage: "covered",
        scopeId: "org-hq-finance",
        scopeLabel: "总部财务",
        permissionVersion: "v1",
        projectedAt: "2026-08-14T08:00:00.000Z",
        sourceWatermark: "2026-08-14T07:00:00.000Z",
        amountBasis: "NET",
        businessType: "GOODS_SERVICE",
        rowCount: 1,
    },
}

export function makeQuery(
    overrides: Partial<ProfitLossQuery> = {},
): ProfitLossQuery {
    return {
        from: "2026-08-01",
        to: "2026-08-14",
        periodBasis: "sales_revenue_recognition_date",
        scopeId: "org-hq-finance",
        coverage: "covered",
        dimension: "sales_order",
        sort: "actualProfitLossNet:asc",
        pageSize: 20,
        ...overrides,
    }
}
