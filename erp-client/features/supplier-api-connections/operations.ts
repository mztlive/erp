import type { ResultState } from "@/components/business/feedback"
import type { FormalOutcome } from "@/features/supplier-api-connections/types"

function outcomeToResult(outcome: FormalOutcome): ResultState {
    if (outcome.status === "succeeded") {
        return {
            status: "succeeded",
            title: outcome.title,
            description: outcome.message,
            reference: outcome.reference ?? outcome.auditEventId,
            facts: outcome.facts,
        }
    }
    if (outcome.status === "processing") {
        return {
            status: "processing",
            title: outcome.title,
            description: outcome.message,
            reference: outcome.jobNo,
            jobId: outcome.jobId,
            jobNo: outcome.jobNo,
        }
    }
    if (outcome.status === "unknown") {
        return {
            status: "unknown",
            title: outcome.title,
            description: outcome.message,
        }
    }
    return {
        status: outcome.status,
        title: outcome.title,
        description: outcome.message,
        reference: outcome.reference,
    }
}

function newIdempotencyKey(prefix: string) {
    return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
}

export { newIdempotencyKey, outcomeToResult }
