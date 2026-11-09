"use client"

import { useMemo } from "react"
import { useQueries, type UseQueryResult } from "@tanstack/react-query"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { isApiError } from "@/lib/api/errors"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { fetchWorkspaceTaskMaterials } from "../api/task-materials"
import type { WorkspaceDocumentFacts } from "../lib/document-facts"
import type { WorkspaceWorkItem } from "../types"

/** 仅订阅材料展示状态，保持表格数据在无变化时的引用稳定。 */
function combineMaterials(
    results: UseQueryResult<WorkspaceDocumentFacts | null>[],
) {
    return results.map(({ data, error, isError, isPending, refetch }) => ({
        data,
        error,
        isError,
        isPending,
        refetch,
    }))
}

/** 同一材料引用只查一次；各行独立失败，账号或授权版本变化后不得沿用旧材料。 */
export function useWorkspaceListMaterials(items: readonly WorkspaceWorkItem[]) {
    const { data: profile } = useAccountProfileQuery()
    const references = useMemo(
        () =>
            items.map((item) =>
                JSON.stringify([
                    item.workItemType,
                    item.businessObjectType,
                    item.businessObjectId,
                    item.subjectVersion,
                    item.approvalProcessInstanceId ?? item.approval?.instanceId,
                ]),
            ),
        [items],
    )
    const unique = useMemo(
        () => [
            ...new Map(
                items.map((item, index) => [references[index], item]),
            ).entries(),
        ],
        [items, references],
    )
    const queries = useQueries({
        combine: combineMaterials,
        queries: unique.map(([reference, item]) => ({
            queryKey: [
                ...queryKeyRoots.workspaceHome,
                "list-materials",
                profile?.userid,
                profile?.policy_version,
                profile?.organization_version,
                profile ? [...profile.permissions].sort() : [],
                reference,
            ],
            queryFn: () => fetchWorkspaceTaskMaterials(item),
            enabled: Boolean(profile),
            retry: false,
            staleTime: 0,
            refetchInterval: 60_000,
        })),
    })
    return useMemo(() => {
        const byReference = new Map(
            unique.map(([reference], index) => [reference, queries[index]]),
        )
        return items.map((item, index) => {
            const query = byReference.get(references[index])!
            const facts = profile && !query.isError ? query.data : undefined
            if (!facts) {
                const forbidden =
                    isApiError(query.error) && query.error.status === 403
                return {
                    item,
                    materialNotice:
                        !profile || query.isPending
                            ? "资料加载中…"
                            : forbidden
                              ? "无权限查看业务资料"
                              : query.isError
                                ? "资料加载失败"
                                : "请打开详情查看资料",
                    retryMaterials:
                        query.isError && !forbidden
                            ? () => {
                                  void query.refetch()
                              }
                            : undefined,
                }
            }
            return {
                item: {
                    ...item,
                    stableNumber: facts.documentNumber || item.stableNumber,
                    counterpartyName: facts.counterparty,
                    objectTitle: facts.counterparty || item.objectTitle,
                    listSummary: facts.listSummary,
                    summarySections: facts.sections,
                    briefLines: facts.lines,
                    briefMoreCount: facts.moreCount,
                    documentSummaryResolved: true,
                },
                materialNotice: undefined,
                retryMaterials: undefined,
            }
        })
    }, [items, profile, queries, references, unique])
}
