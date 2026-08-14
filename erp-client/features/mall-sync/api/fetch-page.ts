/**
 * 读路径：商城同步页面聚合查询 + 商城来源系统解析。
 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type {
    BackendCursor,
    BackendJob,
    BackendMappingTask,
    BackendReconItem,
    BackendReconJob,
    BackendSnapshot,
    BackendSourceSystem,
} from "@/features/mall-sync/api/backend-dtos"
import {
    instantToIso,
    toJobRow,
    toMappingTask,
    toSnapshotRow,
} from "@/features/mall-sync/api/mappers"
import {
    buildMetrics,
    mapReconJobStatus,
    toDifference,
} from "@/features/mall-sync/api/recon-mappers"
import type {
    MallSyncPageView,
    MallSyncViewName,
    OwnershipStage,
    ReconciliationBatch,
    ReconciliationDifference,
} from "@/features/mall-sync/types"

/** Query input（契约保持） */
export type MallSyncQueryInput = {
    view: MallSyncViewName
    sourceUnavailable?: boolean
    q?: string
    jobId?: string
    snapshotId?: string
    mappingTaskId?: string
    workItemId?: string
    differenceId?: string
    queueContextId?: string
    owner?: "mine" | "all"
    mappingType?: string
}

export async function resolveMallSourceSystemId(): Promise<{
    id: string
    code: string
    name: string
    environmentLabel: string
    stage: OwnershipStage
} | null> {
    const page = await apiGet<Page<BackendSourceSystem>>(
        "/admin/source-systems",
        {
            page: 1,
            page_size: 50,
            system_type: "MALL",
        },
    )
    const mall =
        page.items.find((s) => s.status === "active") ?? page.items[0] ?? null
    if (!mall) return null
    return {
        id: mall.id,
        code: mall.code,
        name: mall.name,
        environmentLabel: mall.status === "active" ? "启用" : "停用",
        // 阶段未由服务端明确返回时按封存处理，禁止客户端猜测仍可写。
        stage: mall.mall_sync_stage ?? "ARCHIVED",
    }
}

export async function fetchMallSyncPage(
    input: MallSyncQueryInput,
): Promise<MallSyncPageView> {
    const listParams = { page: 1, page_size: 50 as const }

    const [source, jobsPage, snapshotsPage, mappingPage, reconPage] =
        await Promise.all([
            resolveMallSourceSystemId(),
            apiGet<Page<BackendJob>>("/admin/mall-sales-sync-jobs", listParams),
            apiGet<Page<BackendSnapshot>>(
                "/admin/mall-sales-order-snapshots",
                listParams,
            ),
            apiGet<Page<BackendMappingTask>>(
                "/admin/master-mapping-tasks",
                listParams,
            ),
            apiGet<Page<BackendReconJob>>(
                "/admin/mall-sales-reconciliation-jobs",
                listParams,
            ),
        ])

    let explicitMappingTask: BackendMappingTask | undefined
    if (input.mappingTaskId) {
        explicitMappingTask = await apiGet<BackendMappingTask>(
            `/admin/master-mapping-tasks/${encodeURIComponent(input.mappingTaskId)}`,
            input.workItemId ? { work_item_id: input.workItemId } : undefined,
        )
    }

    let cursor: BackendCursor | null = null
    if (source) {
        try {
            cursor = await apiGet<BackendCursor>(
                "/admin/mall-sales-sync-cursors",
                {
                    source_system_id: source.id,
                },
            )
        } catch {
            cursor = null
        }
    }

    const latestRecon = reconPage.items[0] ?? null
    let differences: ReconciliationDifference[] = []
    if (latestRecon) {
        const itemsPage = await apiGet<Page<BackendReconItem>>(
            `/admin/mall-sales-reconciliation-jobs/${latestRecon.id}/items`,
            listParams,
        )
        differences = itemsPage.items.map(toDifference)
    }

    const jobRows = jobsPage.items.map(toJobRow)
    const jobNoById = new Map(jobRows.map((j) => [j.jobId, j.jobNo]))
    let snapshots = snapshotsPage.items.map((s) => toSnapshotRow(s, jobNoById))
    const snapById = new Map(snapshotsPage.items.map((s) => [s.id, s]))
    const mappingItems = [...mappingPage.items]
    if (explicitMappingTask) {
        const index = mappingItems.findIndex(
            (task) => task.id === explicitMappingTask.id,
        )
        if (index >= 0) mappingItems[index] = explicitMappingTask
        else mappingItems.push(explicitMappingTask)
    }
    let mappingTasks = mappingItems.map((task) => toMappingTask(task, snapById))

    // Client-side q filter only when backend has no free-text search for this surface
    if (input.q?.trim()) {
        const q = input.q.trim().toUpperCase()
        snapshots = snapshots.filter(
            (s) =>
                s.externalOrderNo.toUpperCase().includes(q) ||
                s.syncJobNo.toUpperCase().includes(q),
        )
        mappingTasks = mappingTasks.filter((t) =>
            t.externalOrderNo.toUpperCase().includes(q),
        )
    }
    if (input.mappingType) {
        mappingTasks = mappingTasks.filter(
            (t) => t.mappingType === input.mappingType,
        )
    }

    const recon: ReconciliationBatch | null = latestRecon
        ? {
              jobId: latestRecon.id,
              jobNo: latestRecon.job_no,
              boundaryLabel:
                  instantToIso(latestRecon.source_list_as_of) ??
                  latestRecon.job_no,
              mallCount: latestRecon.source_count,
              erpCount: latestRecon.erp_count,
              differenceCount: latestRecon.difference_count,
              ...mapReconJobStatus(latestRecon.status),
              startedAt: instantToIso(latestRecon.started_at) ?? "",
              finishedAt: instantToIso(latestRecon.finished_at ?? undefined),
              differences,
          }
        : null

    const watermarkIso = instantToIso(cursor?.high_water_updated_at)
    const latestSuccessJob = jobRows.find((j) => j.status === "SUCCEEDED")
    const lagSeconds =
        cursor?.high_water_updated_at != null
            ? Math.max(
                  0,
                  Math.floor(Date.now() / 1000) - cursor.high_water_updated_at,
              )
            : undefined

    const stage: OwnershipStage = source?.stage ?? "ARCHIVED"
    const sourceUnavailable = !source

    const metrics = buildMetrics(jobRows, mappingTasks, recon, lagSeconds)

    const selectedJob =
        jobRows.find((j) => j.jobId === input.jobId) ??
        (input.view === "jobs" ? jobRows[0] : undefined)
    const selectedSnapshot =
        snapshots.find((s) => s.snapshotId === input.snapshotId) ??
        (input.view === "snapshots" ? snapshots[0] : undefined)
    const selectedMappingTask = input.mappingTaskId
        ? mappingTasks.find(
              (task) =>
                  task.mappingTaskId === input.mappingTaskId &&
                  (!input.workItemId ||
                      (task.ownerRoutingState === "CONFIGURED" &&
                          task.workItem.workItemId === input.workItemId)),
          )
        : input.workItemId
          ? mappingTasks.find(
                (task) =>
                    task.ownerRoutingState === "CONFIGURED" &&
                    task.workItem.workItemId === input.workItemId,
            )
          : input.view === "mapping"
            ? mappingTasks[0]
            : undefined

    const selectedDifference = recon?.differences.find(
        (d) => d.differenceId === input.differenceId,
    )

    let emptyReason: MallSyncPageView["emptyReason"]
    if (input.view === "mapping" && mappingTasks.length === 0) {
        emptyReason = "NO_TASKS"
    } else if (!source) {
        emptyReason = "NO_SCOPE"
    }

    const asOf =
        watermarkIso ??
        latestSuccessJob?.finishedAt ??
        latestSuccessJob?.startedAt ??
        instantToIso(jobsPage.items[0]?.created_at) ??
        instantToIso(0)

    return {
        context: {
            sourceSystem: source ?? {
                id: "",
                code: "",
                name: "未配置商城来源",
                environmentLabel: "—",
            },
            ownership: {
                businessType: "VOUCHER",
                stage,
                originSystemSummary: "MALL",
                syncDirection: "MALL_TO_ERP_COMMERCIAL_FACT",
                firstPhasePollingEnabled: Boolean(source),
                mallWriteBoundary:
                    "商城开单商业记录（可继续销售/制卡/绑定/激活/消费）",
                erpWriteBoundary: "ERP 只读接收商业数据；不向商城回写商业修改",
            },
            freshness: {
                currentWatermark: watermarkIso,
                latestSuccessfulJobAt: latestSuccessJob?.finishedAt,
                sourceSafeTime: watermarkIso,
                syncLagSeconds: lagSeconds,
                viewProjectedAt: asOf ?? "",
            },
            metrics,
            sourceUnavailable,
            sourceUnavailableMessage: sourceUnavailable
                ? "未找到启用的商城来源系统；请先在来源系统中登记 MALL 类型来源。"
                : undefined,
            hasSourceScope: Boolean(source),
            scheduledIncrementalNote:
                "系统定时增量按调度契约独立运行；授权管理员可直接提交带理由的人工增量。",
        },
        jobs: jobRows,
        snapshots,
        mappingTasks,
        reconciliation: recon,
        history: [],
        selectedJob,
        selectedSnapshot,
        selectedMappingTask,
        selectedDifference,
        emptyReason,
    }
}
