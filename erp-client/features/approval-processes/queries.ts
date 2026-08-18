"use client"

import {
    useMutation,
    useQuery,
    useQueryClient,
    type QueryClient,
} from "@tanstack/react-query"

import {
    createDefinitionDraft,
    fetchDefinitionCatalog,
    fetchDefinitionDetail,
    fetchDefinitionVersions,
    fetchEligibleAssignees,
    publishDefinition,
    replaceDefinitionNodes,
    retireDefinition,
} from "./api"
import { unwrapResult } from "./result"
import type {
    CreateDefinitionDraftRequest,
    DefinitionDetailView,
    DefinitionLockRequest,
    DocumentType,
    ReplaceDefinitionNodesRequest,
} from "./types"

/**
 * 审批流程 Query Key。成功后按定义 ID、单据类型精确失效，不得清空 QueryClient。
 */
export const approvalProcessKeys = {
    all: ["approvalProcesses"] as const,
    catalog: () => [...approvalProcessKeys.all, "catalog"] as const,
    versions: (documentType: DocumentType) =>
        [...approvalProcessKeys.all, "versions", documentType] as const,
    detail: (definitionId: string) =>
        [...approvalProcessKeys.all, "detail", definitionId] as const,
    eligibleAssignees: (documentType: DocumentType, search: string) =>
        [
            ...approvalProcessKeys.all,
            "eligibleAssignees",
            documentType,
            search,
        ] as const,
}

/**
 * 按返回的定义精确失效目录、版本和详情。
 *
 * @param queryClient QueryClient
 * @param detail 变更后的定义
 */
export const invalidateDefinitionQueries = async (
    queryClient: QueryClient,
    detail: Pick<DefinitionDetailView, "definition_id" | "document_type">,
): Promise<void> => {
    await Promise.all([
        queryClient.invalidateQueries({
            queryKey: approvalProcessKeys.catalog(),
        }),
        queryClient.invalidateQueries({
            queryKey: approvalProcessKeys.versions(detail.document_type),
        }),
        queryClient.invalidateQueries({
            queryKey: approvalProcessKeys.detail(detail.definition_id),
        }),
    ])
}

/**
 * 读取固定单据类型目录。背景刷新保留已有行，不替换为整页骨架。
 */
export function useDefinitionCatalogQuery() {
    return useQuery({
        queryKey: approvalProcessKeys.catalog(),
        queryFn: async () => unwrapResult(await fetchDefinitionCatalog()),
        placeholderData: (previous) => previous,
    })
}

/**
 * 读取某类型的定义版本历史。
 *
 * @param documentType 固定单据类型
 * @param enabled 是否查询
 */
export function useDefinitionVersionsQuery(
    documentType: DocumentType | null,
    enabled = true,
) {
    return useQuery({
        queryKey: documentType
            ? approvalProcessKeys.versions(documentType)
            : [...approvalProcessKeys.all, "versions", "idle"],
        queryFn: async () => {
            if (!documentType) throw new Error("未选择单据类型")
            return unwrapResult(await fetchDefinitionVersions(documentType))
        },
        enabled: Boolean(documentType) && enabled,
        placeholderData: (previous) => previous,
    })
}

/**
 * 读取定义详情。
 *
 * @param definitionId 定义主键
 * @param enabled 是否查询
 */
export function useDefinitionDetailQuery(
    definitionId: string | null,
    enabled = true,
) {
    return useQuery({
        queryKey: definitionId
            ? approvalProcessKeys.detail(definitionId)
            : [...approvalProcessKeys.all, "detail", "idle"],
        queryFn: async () => {
            if (!definitionId) throw new Error("未选择审批流程")
            return unwrapResult(await fetchDefinitionDetail(definitionId))
        },
        enabled: Boolean(definitionId) && enabled,
        placeholderData: (previous) => previous,
    })
}

/**
 * 搜索定义期可选审批人。
 *
 * @param documentType 固定单据类型
 * @param search 检索词
 * @param enabled 是否查询
 */
export function useEligibleAssigneesQuery(
    documentType: DocumentType | null,
    search: string,
    enabled = true,
) {
    const normalized = search.trim()
    return useQuery({
        queryKey: documentType
            ? approvalProcessKeys.eligibleAssignees(documentType, normalized)
            : [...approvalProcessKeys.all, "eligibleAssignees", "idle"],
        queryFn: async () => {
            if (!documentType) throw new Error("未选择单据类型")
            return unwrapResult(
                await fetchEligibleAssignees(documentType, normalized),
            )
        },
        enabled: Boolean(documentType) && enabled,
        placeholderData: (previous) => previous,
        staleTime: 15_000,
    })
}

/**
 * 创建草稿。成功后精确失效目录、版本和详情。
 */
export function useCreateDefinitionDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async (request: CreateDefinitionDraftRequest) =>
            unwrapResult(await createDefinitionDraft(request)),
        onSuccess: (detail) => invalidateDefinitionQueries(queryClient, detail),
    })
}

/**
 * 整组替换草稿节点。
 */
export function useReplaceDefinitionNodesMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async ({
            definitionId,
            request,
        }: {
            definitionId: string
            request: ReplaceDefinitionNodesRequest
        }) => unwrapResult(await replaceDefinitionNodes(definitionId, request)),
        onSuccess: (detail) => invalidateDefinitionQueries(queryClient, detail),
    })
}

/**
 * 发布草稿。
 */
export function usePublishDefinitionMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async ({
            definitionId,
            request,
        }: {
            definitionId: string
            request: DefinitionLockRequest
        }) => unwrapResult(await publishDefinition(definitionId, request)),
        onSuccess: (detail) => invalidateDefinitionQueries(queryClient, detail),
    })
}

/**
 * 退役已发布定义。
 */
export function useRetireDefinitionMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async ({
            definitionId,
            request,
        }: {
            definitionId: string
            request: DefinitionLockRequest
        }) => unwrapResult(await retireDefinition(definitionId, request)),
        onSuccess: (detail) => invalidateDefinitionQueries(queryClient, detail),
    })
}
