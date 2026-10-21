"use client"

import { useQuery } from "@tanstack/react-query"
import {
    fetchExpenseSource,
    fetchExpenseSources,
    fetchInquiryExpenseCandidates,
    type ExpenseSourceQuery,
} from "../api/expense-sources"

const expenseSourceKeys = ["purchase-orders", "expense-sources"] as const
export function useExpenseSourcesQuery(
    input: ExpenseSourceQuery,
    enabled: boolean,
) {
    return useQuery({
        queryKey: [...expenseSourceKeys, "list", input],
        queryFn: () => fetchExpenseSources(input),
        enabled,
        staleTime: 0,
    })
}

export function useInquiryExpenseCandidatesQuery(
    salesOrderId: string,
    enabled: boolean,
) {
    return useQuery({
        queryKey: [...expenseSourceKeys, "inquiry-fees", salesOrderId],
        queryFn: () => fetchInquiryExpenseCandidates(salesOrderId),
        enabled: enabled && Boolean(salesOrderId),
        staleTime: 0,
    })
}
export function useExpenseSourceQuery(
    id: string | undefined,
    enabled: boolean,
) {
    return useQuery({
        queryKey: [...expenseSourceKeys, "detail", id],
        queryFn: () => fetchExpenseSource(id!),
        enabled: enabled && Boolean(id),
        staleTime: 0,
    })
}
