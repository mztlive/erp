import type { ResultState } from "@/components/business/feedback"
import type { FormalOutcome } from "@/features/supplier-settlements/types"

function outcomeToResult(outcome: FormalOutcome): ResultState {
    const w12Href = outcome.payableNo
        ? `/finance/supplier-accounts?view=payable&sourceType=SUPPLIER_SETTLEMENT&q=${encodeURIComponent(outcome.payableNo)}`
        : undefined
    if (outcome.status === "succeeded") {
        return {
            status: "succeeded",
            title: outcome.title,
            description: outcome.message,
            reference: outcome.reference ?? outcome.payableNo,
            facts: outcome.facts,
            payableNo: outcome.payableNo,
            w12Href,
        }
    }
    if (outcome.status === "unknown") {
        return {
            status: "unknown",
            title: outcome.title,
            description: outcome.message,
        }
    }
    if (outcome.status === "rejected") {
        return {
            status: "rejected",
            title: outcome.title,
            description: outcome.message,
            reference: outcome.reference,
            facts: outcome.facts,
        }
    }
    return {
        status: outcome.status === "blocked" ? "blocked" : "failed",
        title: outcome.title,
        description: outcome.message,
        reference: outcome.reference,
        facts: outcome.facts,
    }
}

function newKey(prefix: string) {
    return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
}

function blockerOf(
    blockers: { action: string; message: string; code: string }[],
    action: string,
) {
    return blockers.find((b) => b.action === action)
}

export { blockerOf, newKey, outcomeToResult }
