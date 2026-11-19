"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { useQuery } from "@tanstack/react-query"

import {
    fetchWorkspaceDocumentFacts,
    shouldLoadDocumentFacts,
    type WorkspaceDocumentFacts,
} from "@/features/workspace/lib/document-facts"
import type { WorkspaceWorkItem } from "@/features/workspace/types"

/**
 * 工作台按任务绑定的身份和版本独立读取资料；材料拒绝不影响任务。
 */
export function useWorkspaceDocumentFacts(item: WorkspaceWorkItem) {
    const { data: profile } = useAccountProfileQuery()
    const hasSummary = Boolean(
        item.documentSummaryResolved ||
        (item.summarySections && item.summarySections.length > 0) ||
        (item.briefLines && item.briefLines.length > 0),
    )
    const approval =
        item.workItemType === "DOCUMENT_APPROVAL" ||
        item.workItemType === "APPROVAL_INSTANCE" ||
        item.workItemType === "APPROVAL_REWORK"
    const enabled =
        !approval &&
        shouldLoadDocumentFacts({
            businessObjectType: item.businessObjectType,
            hasSummary,
        })
    const query = useQuery({
        queryKey: [
            "workspace-document-facts",
            profile?.userid,
            profile?.policy_version,
            profile?.organization_version,
            item.workItemType,
            item.businessObjectType,
            item.businessObjectId,
            item.subjectVersion,
        ],
        queryFn: () => fetchWorkspaceDocumentFacts(item),
        enabled: enabled && Boolean(profile),
        staleTime: 0,
        retry: false,
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
        : approval
          ? null
          : query.isError
            ? null
            : (query.data ?? null)
    return {
        facts,
        isPending: enabled && query.isPending,
        isError: enabled && query.isError,
        error: query.error,
        refetch: query.refetch,
    }
}
