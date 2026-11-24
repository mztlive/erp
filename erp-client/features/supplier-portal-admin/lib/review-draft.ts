import { z } from "zod"
import {
    reviewFormSchema,
    type ReviewFormValues,
} from "../components/review-sections/types"
import type { FormalCommandIdentity } from "@/lib/formal-command"

export type ReviewDecision = "approve" | "return"
export type ReviewCommand = Record<string, unknown> & {
    decision: ReviewDecision
    work_item_id: string
    expected_version: number
}

const draftSchema = z.object({
    version: z.number(),
    values: reviewFormSchema,
    reportedConfirmed: z.boolean(),
    search: z.string(),
})

const attemptSchema = z.object({
    idempotencyKey: z.string().min(1),
    payload: z.looseObject({
        decision: z.enum(["approve", "return"]),
        work_item_id: z.string(),
        work_item_version: z.number(),
        expected_version: z.number(),
    }),
})

export const PORTAL_REVIEW_STORAGE_EVENT =
    "erp-supplier-portal-review-storage-change"

function notifyPortalReviewStorage(key: string) {
    if (typeof window === "undefined") return
    window.dispatchEvent(
        new CustomEvent(PORTAL_REVIEW_STORAGE_EVENT, { detail: key }),
    )
}

export function portalReviewStorageKey(
    actorId: string,
    applicationId: string,
    taskId: string,
) {
    return `erp.supplier-portal-review.v1:${[actorId, applicationId, taskId].map(encodeURIComponent).join(":")}`
}

function readLocal<T>(key: string, schema: z.ZodType<T>): T | undefined {
    try {
        const raw = localStorage.getItem(key)
        return raw ? schema.safeParse(JSON.parse(raw)).data : undefined
    } catch {
        return undefined
    }
}

export function readPortalReviewDraft(key: string, version: number) {
    const draft = readLocal(key, draftSchema)
    return draft?.version === version ? draft : undefined
}

export function savePortalReviewDraft(
    key: string,
    draft: {
        version: number
        values: ReviewFormValues
        reportedConfirmed: boolean
        search: string
    },
) {
    localStorage.setItem(key, JSON.stringify(draft))
    notifyPortalReviewStorage(key)
}

export function readPortalReviewAttempt(
    key: string,
    taskId: string,
): FormalCommandIdentity<ReviewCommand> | undefined {
    const attempt = readLocal(`${key}:attempt`, attemptSchema)
    return attempt?.payload.work_item_id === taskId ? attempt : undefined
}

export function savePortalReviewAttempt(
    key: string,
    attempt: FormalCommandIdentity<ReviewCommand>,
) {
    localStorage.setItem(`${key}:attempt`, JSON.stringify(attempt))
    notifyPortalReviewStorage(`${key}:attempt`)
}

export function clearPortalReviewAttempt(
    key: string,
    taskId: string,
    idempotencyKey: string,
) {
    if (
        readPortalReviewAttempt(key, taskId)?.idempotencyKey === idempotencyKey
    ) {
        removeLocal(`${key}:attempt`)
        notifyPortalReviewStorage(`${key}:attempt`)
    }
}

export function clearPortalReviewDraft(key: string, version: number) {
    if (readPortalReviewDraft(key, version)) {
        removeLocal(key)
        notifyPortalReviewStorage(key)
    }
}

function removeLocal(key: string) {
    try {
        localStorage.removeItem(key)
    } catch {
        // A storage failure must not change a decision already settled by the server.
    }
}
