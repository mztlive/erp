import { apiGet, apiPost, apiPut } from "@/lib/api"
import { createApiError } from "@/lib/api/errors"

import {
    parseCatalog,
    parseDefinitionDetail,
    parseEligibleAssignees,
    parseVersions,
} from "./parse"
import { fromPromise, type ResultAsync } from "./result"
import type {
    CreateDefinitionDraftRequest,
    DefinitionCatalogItem,
    DefinitionDetailView,
    DefinitionLockRequest,
    DefinitionVersionItem,
    DocumentType,
    EligibleAssignee,
    ReplaceDefinitionNodesRequest,
} from "./types"
import { assertWritePayloadSafe } from "./write-payload"
import type { ApiError } from "./result"

const requireDetail = (value: unknown): DefinitionDetailView => {
    const detail = parseDefinitionDetail(value)
    if (!detail) {
        throw createApiError({
            kind: "Parse",
            message: "系统返回的审批流程数据无法读取，请稍后重试。",
            responseData: value,
        })
    }
    return detail
}

/**
 * 读取固定 20 行审批流程目录。
 *
 * @returns 目录行 Result
 */
export const fetchDefinitionCatalog = (): ResultAsync<
    DefinitionCatalogItem[],
    ApiError
> =>
    fromPromise(async () =>
        parseCatalog(
            await apiGet<unknown>("/admin/approval-processes/catalog"),
        ),
    )

/**
 * 读取某单据类型的定义版本列表。
 *
 * @param documentType 固定单据类型
 */
export const fetchDefinitionVersions = (
    documentType: DocumentType,
): ResultAsync<DefinitionVersionItem[], ApiError> =>
    fromPromise(async () =>
        parseVersions(
            await apiGet<unknown>(
                `/admin/approval-processes/${encodeURIComponent(documentType)}/versions`,
            ),
        ),
    )

/**
 * 读取定义图详情。
 *
 * @param definitionId 定义主键
 */
export const fetchDefinitionDetail = (
    definitionId: string,
): ResultAsync<DefinitionDetailView, ApiError> =>
    fromPromise(async () =>
        requireDetail(
            await apiGet<unknown>(
                `/admin/approval-process-definitions/${encodeURIComponent(definitionId)}`,
            ),
        ),
    )

/**
 * 按定义期规则搜索可选审批人，不在浏览器过滤全量账号。
 *
 * @param documentType 固定单据类型
 * @param search 姓名或账号检索
 */
export const fetchEligibleAssignees = (
    documentType: DocumentType,
    search: string,
): ResultAsync<EligibleAssignee[], ApiError> =>
    fromPromise(async () =>
        parseEligibleAssignees(
            await apiGet<unknown>(
                `/admin/approval-processes/${encodeURIComponent(documentType)}/eligible-assignees`,
                {
                    search: search.trim() || undefined,
                    limit: 20,
                },
            ),
        ),
    )

/**
 * 创建更高版本草稿。请求不得携带源定义 ID。
 *
 * @param request 创建草稿请求
 */
export const createDefinitionDraft = (
    request: CreateDefinitionDraftRequest,
): ResultAsync<DefinitionDetailView, ApiError> =>
    fromPromise(async () => {
        assertWritePayloadSafe(request)
        return requireDetail(
            await apiPost<unknown>(
                "/admin/approval-process-definitions/drafts",
                request,
            ),
        )
    })

/**
 * 整组替换草稿节点。新增节点无 key，已有节点只提交 node_id。
 *
 * @param definitionId 草稿定义主键
 * @param request 节点写请求
 */
export const replaceDefinitionNodes = (
    definitionId: string,
    request: ReplaceDefinitionNodesRequest,
): ResultAsync<DefinitionDetailView, ApiError> =>
    fromPromise(async () => {
        assertWritePayloadSafe(request)
        return requireDetail(
            await apiPut<unknown>(
                `/admin/approval-process-definitions/${encodeURIComponent(definitionId)}/nodes`,
                request,
            ),
        )
    })

/**
 * 发布草稿并退役旧版本。
 *
 * @param definitionId 草稿定义主键
 * @param request 锁版本与新幂等键
 */
export const publishDefinition = (
    definitionId: string,
    request: DefinitionLockRequest,
): ResultAsync<DefinitionDetailView, ApiError> =>
    fromPromise(async () => {
        assertWritePayloadSafe(request)
        return requireDetail(
            await apiPost<unknown>(
                `/admin/approval-process-definitions/${encodeURIComponent(definitionId)}/publish`,
                request,
            ),
        )
    })

/**
 * 退役当前已发布定义。
 *
 * @param definitionId 已发布定义主键
 * @param request 锁版本与新幂等键
 */
export const retireDefinition = (
    definitionId: string,
    request: DefinitionLockRequest,
): ResultAsync<DefinitionDetailView, ApiError> =>
    fromPromise(async () => {
        assertWritePayloadSafe(request)
        return requireDetail(
            await apiPost<unknown>(
                `/admin/approval-process-definitions/${encodeURIComponent(definitionId)}/retire`,
                request,
            ),
        )
    })
