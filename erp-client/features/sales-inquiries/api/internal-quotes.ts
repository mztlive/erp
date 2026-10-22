import { apiGet } from "@/lib/api"
import { inquiryPath } from "../api"
import type { InternalQuote } from "../lib/internal-quotes"

export function fetchInternalQuotes(inquiryId: string, taskId: string) {
    return apiGet<InternalQuote[]>(
        `${inquiryPath(true, inquiryId)}/internal-quotes`,
        { task_id: taskId },
    )
}
