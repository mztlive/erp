import type {
    HistoryBackfillCommandResult,
    HistoryBackfillDetailView,
    HistoryBackfillJobCore,
    HistoryBackfillListView,
    HistoryBackfillReportView,
} from "@/features/history-backfill/types"

export function makeJob(
    overrides: Partial<HistoryBackfillJobCore> = {},
): HistoryBackfillJobCore {
    return {
        id: "job-1",
        jobNo: "HB-1",
        mallId: "mall-1",
        mallName: "测试商城",
        environment: "production",
        cutoverId: "cutover-1",
        requiredHistoryStart: "2026-01-01T00:00:00.000Z",
        rangeStart: "2026-01-01T00:00:00.000Z",
        rangeEnd: "2026-07-01T00:00:00.000Z",
        cutoverAt: "2026-07-01T00:00:00.000Z",
        coverageComplete: true,
        coverageGaps: [],
        processingStatus: "READY",
        reportReviewStatus: "NOT_READY",
        pipelineStage: "VALIDATE_SOURCE",
        formalDownstreamUnlocked: false,
        lockVersion: 3,
        requestedBy: "测试员",
        requestedAt: "2026-01-01T00:00:00.000Z",
        sourceAsOf: "2026-01-01T00:00:00.000Z",
        fulfillmentNote: "历史记录追加写入，不覆盖实时记录",
        scopeNote: "生效范围从范围起点至截止时点（截止时点当天除外）。",
        legacyManualNote: "截止时点前支付只补台账。",
        progress: {
            totalCount: 100,
            processedCount: 60,
            insertedCount: 40,
            deduplicatedCount: 15,
            unattributedCount: 5,
            failedCount: 0,
            lastProgressAt: "2026-01-02T00:00:00.000Z",
        },
        costBasis: [
            {
                basis: "ACTUAL",
                count: 50,
                consumptionAmountGross: "100.00",
                costAmountNet: "80.00",
            },
            {
                basis: "STANDARD",
                count: 30,
                consumptionAmountGross: "60.00",
                costAmountNet: "50.00",
            },
            {
                basis: "NONE",
                count: 20,
                consumptionAmountGross: "40.00",
                costAmountNet: null,
            },
        ],
        coverageRate: "0.9",
        coveragePercent: 90,
        allowedActions: ["VALIDATE_SOURCE", "START"],
        actionBlockers: [],
        idempotencyNamespace: "mall-backfill:job-1",
        ...overrides,
    }
}

export function makeDetailView(
    overrides: Partial<HistoryBackfillDetailView> = {},
): HistoryBackfillDetailView {
    return {
        job: makeJob(),
        items: [],
        totalItems: 0,
        report: undefined,
        queriedAt: "2026-08-14T08:00:00.000Z",
        permissionVersion: "server",
        ...overrides,
    }
}

export function makeListView(
    overrides: Partial<HistoryBackfillListView> = {},
): HistoryBackfillListView {
    return {
        metrics: {
            running: 0,
            unattributed: 0,
            deduplicated: 0,
            noneConsumption: 0,
            failed: 0,
        },
        rows: [],
        totalCount: 0,
        queriedAt: "2026-08-14T08:00:00.000Z",
        createContext: {
            cutoverId: "",
            mallId: "",
            mallName: "",
            environment: "production",
            requiredHistoryStart: "",
            rangeEnd: "",
            cutoverAt: "",
            sourceCoverageStart: "",
            coverageComplete: false,
            coverageGaps: [],
            estimatedFactCount: 0,
            hasOverlappingFormalJob: false,
            canCreateDraft: false,
            blockReasons: [],
        },
        ...overrides,
    }
}

export function makeReport(
    overrides: Partial<HistoryBackfillReportView> = {},
): HistoryBackfillReportView {
    return {
        reportId: "report-1",
        reportVersion: 7,
        generatedAt: "2026-08-01T00:00:00.000Z",
        reviewLabel: "UNCONFIRMED",
        downloadLabel: "回填报告_HB-1",
        schemaVersion: "1",
        ruleVersion: "1",
        rangeStart: "2026-01-01T00:00:00.000Z",
        rangeEnd: "2026-07-01T00:00:00.000Z",
        cutoverAt: "2026-07-01T00:00:00.000Z",
        totalCount: 10,
        totalAmount: "1.00",
        insertedCount: 9,
        deduplicatedCount: 1,
        unattributedCount: 0,
        failedCount: 0,
        costBasis: makeJob().costBasis,
        coverageRate: null,
        unattributedSummaries: [],
        failedSummaries: [],
        operatorLabel: "测试员",
        processingStatus: "COMPLETED",
        reportReviewStatus: "PENDING",
        fullHistoryFinalComplete: false,
        sensitiveRedactionNote: "已脱敏",
        ...overrides,
    }
}

export function makeCommittedResult(
    overrides: Partial<HistoryBackfillCommandResult> = {},
): HistoryBackfillCommandResult {
    return {
        status: "COMMITTED",
        title: "已提交",
        description: "提交成功。",
        jobId: "job-1",
        jobNo: "HB-1",
        operationId: "op-1",
        idempotencyKey: "idem-1",
        nextStep: "下一步",
        ...overrides,
    }
}
