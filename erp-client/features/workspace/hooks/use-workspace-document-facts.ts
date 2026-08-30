"use client"

import { useQuery } from "@tanstack/react-query"

import {
    fetchWorkspaceDocumentFacts,
    shouldLoadDocumentFacts,
    type WorkspaceDocumentFacts,
} from "@/features/workspace/lib/document-facts"
import type { WorkspaceWorkItem } from "@/features/workspace/types"

/**
 * 工作台详情在服务端简报为空时，按单据详情补齐只读事实。
 */
export function useWorkspaceDocumentFacts(item: WorkspaceWorkItem) {
    const hasSummary = Boolean(
        (item.summarySections && item.summarySections.length > 0) ||
        (item.briefLines && item.briefLines.length > 0),
    )
    const enabled = shouldLoadDocumentFacts({
        businessObjectType: item.businessObjectType,
        hasSummary,
    })
    const query = useQuery({
        queryKey: [
            "workspace-document-facts",
            item.businessObjectType,
            item.businessObjectId,
        ],
        queryFn: () =>
            fetchWorkspaceDocumentFacts({
                businessObjectType: item.businessObjectType,
                businessObjectId: item.businessObjectId,
            }),
        enabled,
        staleTime: 60 * 1000,
    })
    const facts: WorkspaceDocumentFacts | null = hasSummary
        ? {
              counterparty: item.counterpartyName,
              impact: item.impactSummary,
              listSummary: item.listSummary,
              sections: item.summarySections ?? [],
              lines: item.briefLines ?? [],
              moreCount: item.briefMoreCount ?? 0,
          }
        : (query.data ?? null)
    return {
        facts,
        isPending: enabled && query.isPending,
        isError: enabled && query.isError,
        error: query.error,
        refetch: query.refetch,
    }
}
