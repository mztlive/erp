"use client"
import { useQuery } from "@tanstack/react-query"
import { fetchInternalQuotes } from "../api/internal-quotes"

export function useInternalQuotes(
    inquiryId: string,
    taskId: string,
    enabled = true,
) {
    return useQuery({
        queryKey: ["sales-inquiries", "internal-quotes", inquiryId, taskId],
        queryFn: () => fetchInternalQuotes(inquiryId, taskId),
        enabled,
    })
}
