import type { ImportStageKey, ImportStageStates } from "@/components/business"
import type { ImportPipelineStage } from "@/features/import-opening/types"
import {
    PIPELINE_STAGE_LABEL,
    PIPELINE_TO_INDICATOR,
} from "@/features/import-opening/types"

export const PIPELINE_ORDER: ImportPipelineStage[] = [
    "RECEIVE",
    "VALIDATE",
    "TRIAL",
    "CONFIRM",
    "APPLY",
    "RESULT",
]

export function buildStageStates(current: ImportPipelineStage): ImportStageStates {
    const currentIdx = PIPELINE_ORDER.indexOf(current)
    const states: {
        [K in ImportStageKey]: {
            status: "pending" | "current" | "complete" | "failed"
            description?: string
        }
    } = {
        upload: { status: "pending" },
        mapping: { status: "pending" },
        validation: { status: "pending" },
        preview: { status: "pending" },
        submission: { status: "pending" },
        result: { status: "pending" },
    }
    for (let i = 0; i < PIPELINE_ORDER.length; i += 1) {
        const stage = PIPELINE_ORDER[i]!
        const key = PIPELINE_TO_INDICATOR[stage]
        let status: "pending" | "current" | "complete" | "failed" = "pending"
        if (i < currentIdx) status = "complete"
        else if (i === currentIdx) status = "current"
        states[key] = {
            status,
            description: PIPELINE_STAGE_LABEL[stage],
        }
    }
    return states
}

export const importStageLabels = {
    upload: PIPELINE_STAGE_LABEL.RECEIVE,
    mapping: PIPELINE_STAGE_LABEL.VALIDATE,
    validation: PIPELINE_STAGE_LABEL.TRIAL,
    preview: PIPELINE_STAGE_LABEL.CONFIRM,
    submission: PIPELINE_STAGE_LABEL.APPLY,
    result: PIPELINE_STAGE_LABEL.RESULT,
}
