import type { ImportStageStates } from "@/components/business"
import type {
    BackfillPipelineStage,
    HistoryBackfillProcessingStatus,
} from "@/features/history-backfill/types"
import {
    PIPELINE_ORDER,
    PIPELINE_STAGE_LABEL,
    PIPELINE_TO_INDICATOR,
} from "@/features/history-backfill/types"

function newRequestId(prefix: string) {
    return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`
}

function buildStageStates(current: BackfillPipelineStage): ImportStageStates {
    const currentIdx = PIPELINE_ORDER.indexOf(current)
    const states: {
        [K in import("@/components/business").ImportStageKey]: {
            status: "pending" | "current" | "complete" | "failed"
            description?: string
        }
    } = {
        upload: { status: "pending", description: PIPELINE_STAGE_LABEL.SCOPE },
        mapping: {
            status: "pending",
            description: PIPELINE_STAGE_LABEL.VALIDATE_SOURCE,
        },
        validation: {
            status: "pending",
            description: PIPELINE_STAGE_LABEL.INGEST,
        },
        preview: {
            status: "pending",
            description: PIPELINE_STAGE_LABEL.ATTRIBUTE,
        },
        submission: {
            status: "pending",
            description: PIPELINE_STAGE_LABEL.REPORT,
        },
        result: { status: "pending", description: PIPELINE_STAGE_LABEL.DONE },
    }
    for (let i = 0; i < PIPELINE_ORDER.length; i += 1) {
        const stage = PIPELINE_ORDER[i]!
        const key = PIPELINE_TO_INDICATOR[stage]
        let status: "pending" | "current" | "complete" | "failed" = "pending"
        if (i < currentIdx) status = "complete"
        else if (i === currentIdx) status = "current"
        states[key] = { status, description: PIPELINE_STAGE_LABEL[stage] }
    }
    return states
}

function mapJobProgressStatus(
    processing: HistoryBackfillProcessingStatus,
): "queued" | "running" | "succeeded" | "partial" | "failed" {
    if (processing === "RUNNING" || processing === "VALIDATING")
        return "running"
    if (processing === "COMPLETED") return "succeeded"
    if (processing === "PARTIAL") return "partial"
    if (processing === "FAILED") return "failed"
    return "queued"
}

export { buildStageStates, mapJobProgressStatus, newRequestId }
