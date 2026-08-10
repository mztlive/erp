"use client"

import { useQuery } from "@tanstack/react-query"

import {
    fetchImportBatchDetail,
    fetchImportBatchList,
    fetchImportIssues,
} from "@/features/import-opening/api"
import type {
    ImportBatchListQuery,
    ImportIssueQuery,
} from "@/features/import-opening/types"

const importOpeningKeys = {
    all: ["import-opening"] as const,
    list: (query: ImportBatchListQuery) =>
        [...importOpeningKeys.all, "list", query] as const,
    detail: (batchId: string) =>
        [...importOpeningKeys.all, "detail", batchId] as const,
    issues: (query: ImportIssueQuery) =>
        [...importOpeningKeys.all, "issues", query] as const,
}

export function useImportBatchListQuery(query: ImportBatchListQuery) {
    return useQuery({
        queryKey: importOpeningKeys.list(query),
        queryFn: () => fetchImportBatchList(query),
    })
}

export function useImportBatchDetailQuery(batchId: string | undefined) {
    return useQuery({
        queryKey: importOpeningKeys.detail(batchId ?? ""),
        queryFn: () => fetchImportBatchDetail({ batchId: batchId! }),
        enabled: Boolean(batchId),
    })
}

export function useImportIssuesQuery(query: ImportIssueQuery, enabled = true) {
    return useQuery({
        queryKey: importOpeningKeys.issues(query),
        queryFn: () => fetchImportIssues(query),
        enabled: enabled && Boolean(query.batchId),
    })
}
