/**
 * 核对（reconciliation）DTO → 视图映射 + 概览指标聚合。纯函数，无 IO。
 */

import type {
    BackendReconItem,
    BackendReconJob,
} from "@/features/mall-sync/api/backend-dtos"
import type {
    MallSyncJobRow,
    MallSyncMetric,
    MappingTaskView,
    ReconciliationBatch,
    ReconciliationDifference,
} from "@/features/mall-sync/types"

export function mapDiffType(
    t: BackendReconItem["difference_type"],
): ReconciliationDifference["differenceType"] {
    switch (t) {
        case "mall_missing":
            return "MALL_MISSING"
        case "erp_missing":
            return "ERP_MISSING"
        case "status_difference":
            return "STATUS"
        case "content_fingerprint_difference":
            return "FINGERPRINT"
        case "duplicate_identity":
            return "DUPLICATE"
        default:
            return "STATUS"
    }
}

export const DIFF_TYPE_LABEL: Record<
    ReconciliationDifference["differenceType"],
    string
> = {
    MALL_MISSING: "商城缺失",
    ERP_MISSING: "ERP 缺失",
    STATUS: "状态差异",
    FINGERPRINT: "内容不一致",
    DUPLICATE: "重复身份",
}

export function mapDiffStatus(
    s: BackendReconItem["status"],
): Pick<ReconciliationDifference, "status" | "statusLabel" | "statusTone"> {
    switch (s) {
        case "pending":
            return {
                status: "OPEN",
                statusLabel: "待处理",
                statusTone: "warning",
            }
        case "backfilling":
            return {
                status: "PULLING",
                statusLabel: "补拉中",
                statusTone: "info",
            }
        case "resolved":
            return {
                status: "RESOLVED",
                statusLabel: "已解决",
                statusTone: "success",
            }
        case "confirmed_no_difference":
            return {
                status: "CONFIRMED",
                statusLabel: "确认无误",
                statusTone: "success",
            }
        default:
            return { status: "OPEN", statusLabel: s, statusTone: "neutral" }
    }
}

export function mapReconJobStatus(
    s: BackendReconJob["status"],
): Pick<ReconciliationBatch, "status" | "statusLabel"> {
    switch (s) {
        case "running":
            return { status: "RUNNING", statusLabel: "运行中" }
        case "completed":
            return { status: "SUCCEEDED", statusLabel: "完成" }
        case "has_difference":
            return { status: "DIFFERENCE", statusLabel: "有差异" }
        case "failed":
            return { status: "FAILED", statusLabel: "失败" }
        default:
            return { status: "FAILED", statusLabel: s }
    }
}

export function toDifference(item: BackendReconItem): ReconciliationDifference {
    const dt = mapDiffType(item.difference_type)
    const st = mapDiffStatus(item.status)
    return {
        differenceId: item.id,
        externalOrderNo: item.external_order_no,
        differenceType: dt,
        differenceTypeLabel: DIFF_TYPE_LABEL[dt],
        status: st.status,
        statusLabel: st.statusLabel,
        statusTone: st.statusTone,
        impactSummary: item.resolution ?? item.source_status_code,
    }
}

export function buildMetrics(
    jobs: MallSyncJobRow[],
    mappingTasks: MappingTaskView[],
    recon: ReconciliationBatch | null,
    lagSeconds?: number,
): MallSyncMetric[] {
    const pendingMapping = mappingTasks.filter(
        (t) => t.mappingTaskStatus === "PENDING",
    ).length
    const failedJobs = jobs.filter(
        (j) => j.status === "FAILED" || j.status === "PARTIAL_FAILED",
    ).length
    return [
        {
            key: "lag",
            label: "同步延迟",
            value:
                lagSeconds != null
                    ? `${Math.max(0, Math.round(lagSeconds / 60))} 分`
                    : "—",
            visible: true,
            targetView: "overview",
        },
        {
            key: "failed",
            label: "失败任务",
            count: failedJobs,
            visible: true,
            targetView: "jobs",
        },
        {
            key: "pending",
            label: "待映射",
            count: pendingMapping,
            visible: true,
            targetView: "mapping",
        },
        {
            key: "recon",
            label: "核对差异",
            count: recon?.differenceCount ?? 0,
            visible: true,
            targetView: "reconciliation",
        },
    ]
}
