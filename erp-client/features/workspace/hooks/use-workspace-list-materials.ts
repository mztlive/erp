"use client"

import { useMemo } from "react"
import {
    useQueries,
    useQuery,
    type UseQueryResult,
} from "@tanstack/react-query"
import {
    fetchApprovalListMaterials,
    type ApprovalListMaterialReference,
} from "@/features/approval-workflow/api/materials"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { isApiError } from "@/lib/api/errors"
import { displayName } from "@/lib/display-name"
import { hasPermission } from "@/lib/permissions"
import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    approvalListMaterialReference,
    usesApprovalListMaterials,
    workspaceApprovalListFacts,
} from "../api/task-materials"
import {
    fetchWorkspaceDocumentFacts,
    WorkspaceMaterialReferenceMismatch,
    workspaceProcurementListSource,
    type WorkspaceDocumentFacts,
} from "../lib/document-facts"
import type { WorkspaceWorkItem } from "../types"

/** 只订阅资料展示使用的查询状态。 */
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

/** 同一提交引用只发送一次，列表排序变化时复用同一批资料。 */
function collectReferences(items: readonly WorkspaceWorkItem[]) {
    const approvals = new Map<string, ApprovalListMaterialReference>()
    const documents = new Map<
        string,
        {
            item: WorkspaceWorkItem
            procurementSource: ReturnType<typeof workspaceProcurementListSource>
        }
    >()
    const rows = items.map((item) => {
        if (usesApprovalListMaterials(item)) {
            const reference = approvalListMaterialReference(item)
            if (!reference) return { item, kind: "missing" } as const
            const key = JSON.stringify(reference)
            approvals.set(key, reference)
            return { item, kind: "approval", key } as const
        }

        const procurementSource = workspaceProcurementListSource(item)
        const key = JSON.stringify([
            item.workItemType,
            item.businessObjectType,
            item.businessObjectId,
            item.subjectVersion,
            item.approvalProcessInstanceId ?? item.approval?.instanceId,
            ...(procurementSource
                ? [item.workItemId, item.taskVersion, item.status]
                : []),
        ])
        documents.set(key, { item, procurementSource })
        return { item, kind: "document", key, procurementSource } as const
    })
    return {
        rows,
        approvals: [...approvals].sort(([a], [b]) => a.localeCompare(b)),
        documents: [...documents],
    }
}

type ListMaterialRow = {
    item: WorkspaceWorkItem
    materialNotice?: string
    retryMaterials?: () => void
}

function rowWithFacts(item: WorkspaceWorkItem, facts: WorkspaceDocumentFacts) {
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
    }
}

/** 全页审批资料一次请求，普通业务资料继续按原业务接口读取。 */
export function useWorkspaceListMaterials(items: readonly WorkspaceWorkItem[]) {
    const { data: profile } = useAccountProfileQuery()
    const { rows, approvals, documents } = useMemo(
        () => collectReferences(items),
        [items],
    )
    const scopeKey = [
        ...queryKeyRoots.workspaceHome,
        "list-materials",
        profile?.userid,
        profile?.policy_version,
        profile?.organization_version,
        profile ? [...profile.permissions].sort() : [],
        profile ? [...profile.role_ids].sort() : [],
    ]
    const {
        data: approvalData,
        error: approvalError,
        isError: approvalFailed,
        isPending: approvalPending,
        refetch: refetchApprovals,
    } = useQuery({
        queryKey: [
            ...scopeKey,
            "approval-batch",
            approvals.map(([key]) => key),
        ],
        queryFn: ({ signal }) =>
            fetchApprovalListMaterials(
                approvals.map(([, reference]) => reference),
                signal,
            ),
        enabled: Boolean(profile) && approvals.length > 0,
        retry: false,
        staleTime: 0,
        refetchInterval: 60_000,
        meta: {
            feedback: {
                subject: "本页审批资料",
                impact: "待办事项仍会显示在列表中。",
            },
        },
    })
    const documentQueries = useQueries({
        combine: combineMaterials,
        queries: documents.map(([key, { item, procurementSource }]) => {
            const number = displayName(
                item.stableNumber,
                item.businessObjectId,
                item.workItemId,
            )
            return {
                queryKey: [...scopeKey, key],
                queryFn: () =>
                    procurementSource
                        ? procurementSource.load(item)
                        : fetchWorkspaceDocumentFacts(item),
                meta: {
                    feedback: {
                        subject:
                            number && number !== "单号待补全"
                                ? `「${number}」的业务资料`
                                : `「${item.workItemTypeLabel}」事项的业务资料`,
                        impact: "此事项仍会显示在列表中。",
                    },
                },
                enabled:
                    Boolean(profile) &&
                    (!procurementSource ||
                        hasPermission(
                            profile?.permissions,
                            procurementSource.permission,
                        )),
                retry: false,
                staleTime: 0,
                refetchInterval: 60_000,
            }
        }),
    })

    return useMemo<ListMaterialRow[]>(() => {
        const approvalResults = new Map(
            approvals.map(([key], index) => [key, approvalData?.[index]]),
        )
        const documentResults = new Map(
            documents.map(([key], index) => [key, documentQueries[index]]),
        )
        const approvalUnavailable =
            isApiError(approvalError) &&
            (approvalError.status === 403 || approvalError.status === 404)
        const retryApprovals = () =>
            void refetchApprovals({ cancelRefetch: false })

        return rows.map((row) => {
            const { item, kind } = row
            if (kind === "missing")
                return { item, materialNotice: "请打开详情查看资料" }
            const { key } = row
            if (kind === "approval") {
                if (!profile || approvalPending)
                    return { item, materialNotice: "资料加载中…" }
                const result = approvalFailed
                    ? undefined
                    : approvalResults.get(key)
                if (result?.status === "available")
                    return rowWithFacts(
                        item,
                        workspaceApprovalListFacts(result.materials),
                    )
                if (approvalUnavailable || result?.status === "unavailable")
                    return { item, materialNotice: "资料不可查看" }
                if (result?.status === "reference_mismatch")
                    return {
                        item,
                        materialNotice: "资料与任务不一致，请刷新任务",
                    }
                return {
                    item,
                    materialNotice: "资料加载失败",
                    retryMaterials: retryApprovals,
                }
            }

            const { procurementSource } = row
            if (
                profile &&
                procurementSource &&
                !hasPermission(
                    profile.permissions,
                    procurementSource.permission,
                )
            ) {
                return { item, materialNotice: "无权查看业务资料" }
            }
            const query = documentResults.get(key)!
            if (profile && !query.isError && query.data)
                return rowWithFacts(item, query.data)
            if (
                query.error instanceof WorkspaceMaterialReferenceMismatch ||
                (procurementSource &&
                    isApiError(query.error) &&
                    query.error.status === 409)
            ) {
                return { item, materialNotice: "资料与任务不一致，请刷新任务" }
            }
            const forbidden =
                isApiError(query.error) && query.error.status === 403
            return {
                item,
                materialNotice:
                    !profile || query.isPending
                        ? "资料加载中…"
                        : forbidden
                          ? "无权查看业务资料"
                          : query.isError
                            ? "资料加载失败"
                            : "请打开详情查看资料",
                retryMaterials:
                    query.isError && !forbidden
                        ? () => void query.refetch()
                        : undefined,
            }
        })
    }, [
        approvalData,
        approvalError,
        approvalFailed,
        approvalPending,
        approvals,
        documentQueries,
        documents,
        profile,
        refetchApprovals,
        rows,
    ])
}
