/**
 * W30 历史消费回填 · 任务列表 / 详情查询
 */

import { apiGet, type Page } from "@/lib/api"
import type {
    HistoryBackfillDetailQuery,
    HistoryBackfillDetailView,
    HistoryBackfillListQuery,
    HistoryBackfillListView,
} from "@/features/history-backfill/types"
import {
    emptyCreateContext,
    itemResultToBackend,
    mapItem,
    mapJobCore,
    processingToBackend,
    toListItem,
    tsToIso,
} from "@/features/history-backfill/api/mapping"
import type {
    BackendItem,
    BackendJob,
    BackendJobDetail,
} from "@/features/history-backfill/api/wire"

export async function fetchHistoryBackfillList(
    query: HistoryBackfillListQuery,
): Promise<HistoryBackfillListView> {
    const queriedAt = new Date().toISOString()

    const pageRes = await apiGet<Page<BackendJob>>(
        "/admin/mall-consumption-backfill-jobs",
        {
            page: query.page,
            page_size: query.pageSize,
            mall_id: query.mallId,
            status: processingToBackend(query.processingStatus),
            sort_by: "created_at",
            sort_dir: "desc",
        },
    )

    let jobs = (pageRes.items ?? []).map(mapJobCore)

    if (query.view === "active") {
        jobs = jobs.filter((j) =>
            [
                "DRAFT",
                "VALIDATING",
                "READY",
                "RUNNING",
                "PARTIAL",
                "FAILED",
            ].includes(j.processingStatus),
        )
    } else if (query.view === "processing_completed") {
        jobs = jobs.filter((j) => j.processingStatus === "COMPLETED")
    } else if (query.view === "report_pending") {
        jobs = jobs.filter(
            (j) =>
                j.reportReviewStatus === "PENDING" ||
                j.reportReviewStatus === "POLICY_NOT_CONFIGURED",
        )
    }

    if (query.environment) {
        jobs = jobs.filter((j) => j.environment === query.environment)
    }
    if (query.q?.trim()) {
        const q = query.q.trim().toLowerCase()
        jobs = jobs.filter(
            (j) =>
                j.jobNo.toLowerCase().includes(q) ||
                j.mallName.toLowerCase().includes(q) ||
                j.id.toLowerCase().includes(q),
        )
    }
    if (query.basis) {
        const basis = query.basis
        jobs = jobs.filter((j) =>
            j.costBasis.some((c) => c.basis === basis && c.count > 0),
        )
    }

    return {
        metrics: {
            running: jobs.filter((j) => j.processingStatus === "RUNNING")
                .length,
            unattributed: 0,
            deduplicated: 0,
            noneConsumption: 0,
            failed: 0,
        },
        rows: jobs.map(toListItem),
        totalCount: pageRes.total ?? jobs.length,
        queriedAt,
        createContext: emptyCreateContext(),
    }
}

export async function fetchHistoryBackfillDetail(
    query: HistoryBackfillDetailQuery,
): Promise<HistoryBackfillDetailView | null> {
    try {
        const detail = await apiGet<BackendJobDetail>(
            `/admin/mall-consumption-backfill-jobs/${encodeURIComponent(query.jobId)}`,
        )
        const job = mapJobCore(detail.job)

        const itemsPage = await apiGet<Page<BackendItem>>(
            "/admin/mall-consumption-backfill-items",
            {
                page: query.page,
                page_size: query.pageSize,
                job_id: query.jobId,
                result: query.results?.[0]
                    ? itemResultToBackend(query.results[0])
                    : undefined,
                cost_basis: query.costBases?.[0],
                sort_by: "created_at",
                sort_dir: "desc",
            },
        )

        let items = (itemsPage.items ?? []).map(mapItem)
        if (query.q?.trim()) {
            const q = query.q.trim().toLowerCase()
            items = items.filter(
                (i) =>
                    i.mallOrderNo.toLowerCase().includes(q) ||
                    i.businessFactKeySummary.toLowerCase().includes(q) ||
                    (i.sourceDocNo?.toLowerCase().includes(q) ?? false),
            )
        }

        return {
            job,
            items,
            totalItems:
                itemsPage.total ?? detail.item_total_count ?? items.length,
            report: detail.job.report_file_id
                ? {
                      reportId: detail.job.report_file_id,
                      reportVersion: 1,
                      generatedAt: tsToIso(detail.job.created_at),
                      reviewLabel:
                          job.reportReviewStatus === "CONFIRMED"
                              ? "CONFIRMED"
                              : "UNCONFIRMED",
                      downloadLabel: `回填报告_${job.jobNo}`,
                      schemaVersion: "1",
                      ruleVersion: "1",
                      rangeStart: job.rangeStart,
                      rangeEnd: job.rangeEnd,
                      cutoverAt: job.cutoverAt,
                      totalCount: job.progress.totalCount,
                      totalAmount: detail.job.total_amount,
                      insertedCount: job.progress.insertedCount,
                      deduplicatedCount: job.progress.deduplicatedCount,
                      unattributedCount: job.progress.unattributedCount,
                      failedCount: job.progress.failedCount,
                      costBasis: job.costBasis,
                      coverageRate: job.coverageRate,
                      unattributedSummaries: [],
                      failedSummaries: [],
                      operatorLabel: job.requestedBy,
                      processingStatus: job.processingStatus,
                      reportReviewStatus: job.reportReviewStatus,
                      fullHistoryFinalComplete: job.coverageComplete,
                      sensitiveRedactionNote:
                          "报告已脱敏：不含卡号/卡密/手机/完整地址/原始报文。",
                  }
                : undefined,
            queriedAt: new Date().toISOString(),
            permissionVersion: "server",
        }
    } catch (err) {
        const status =
            err && typeof err === "object" && "status" in err
                ? (err as { status?: number }).status
                : undefined
        if (status === 404) return null
        throw err
    }
}
