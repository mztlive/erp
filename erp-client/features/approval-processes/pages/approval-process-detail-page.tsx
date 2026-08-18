"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"

import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"

import { CreateDraftDialog } from "../components/create-draft-dialog"
import { DefinitionEditor } from "../components/definition-editor"
import { PublishDialog } from "../components/publish-dialog"
import { RetireDialog } from "../components/retire-dialog"
import { VersionHistory } from "../components/version-history"
import { definitionErrorMessage } from "../errors"
import { configurationStatusLabel, documentTypeLabel } from "../labels"
import { isDocumentType } from "../parse"
import { canPerformCatalogAction, canReadCatalog } from "../permissions"
import {
    useDefinitionCatalogQuery,
    useDefinitionDetailQuery,
    useDefinitionVersionsQuery,
} from "../queries"
import type { DefinitionCatalogItem } from "../types"
import {
    buildDetailSearchParams,
    hasUnknownDetailParams,
    parseDetailSearchParams,
} from "../url-state"

/**
 * 单个单据类型的定义详情、草稿编辑与历史版本。
 */
export function ApprovalProcessDetailPage({
    documentType: rawDocumentType,
}: {
    documentType: string
}) {
    const router = useRouter()
    const searchParams = useSearchParams()
    const profileQuery = useAccountProfileQuery()
    const catalogQuery = useDefinitionCatalogQuery()
    const unknownParams = hasUnknownDetailParams(
        new URLSearchParams(searchParams.toString()),
    )
    const urlState = React.useMemo(
        () =>
            parseDetailSearchParams(
                new URLSearchParams(searchParams.toString()),
            ),
        [searchParams],
    )
    const documentType = isDocumentType(rawDocumentType)
        ? rawDocumentType
        : null
    const catalogItem =
        catalogQuery.data?.find(
            (item) => item.document_type === documentType,
        ) ?? null
    const versionsQuery = useDefinitionVersionsQuery(
        documentType,
        Boolean(documentType) &&
            catalogItem?.approval_requirement === "PROCESS_REQUIRED",
    )
    const published = versionsQuery.data?.find(
        (item) => item.status === "PUBLISHED",
    )
    const draft = versionsQuery.data?.find((item) => item.status === "DRAFT")
    const historyTarget =
        urlState.view === "history" && urlState.version
            ? versionsQuery.data?.find(
                  (item) => item.definition_version === urlState.version,
              )
            : undefined
    const targetId =
        urlState.view === "draft"
            ? (draft?.definition_id ?? null)
            : urlState.view === "history"
              ? (historyTarget?.definition_id ??
                published?.definition_id ??
                null)
              : (published?.definition_id ?? draft?.definition_id ?? null)
    const detailQuery = useDefinitionDetailQuery(targetId)
    const [lockVersion, setLockVersion] = React.useState("")
    const [publishOpen, setPublishOpen] = React.useState(false)
    const [retireOpen, setRetireOpen] = React.useState(false)
    const [createOpen, setCreateOpen] = React.useState(false)

    React.useEffect(() => {
        if (detailQuery.data) {
            setLockVersion(detailQuery.data.definition_lock_version)
        }
    }, [detailQuery.data])

    React.useEffect(() => {
        if (!profileQuery.data) return
        if (canReadCatalog(profileQuery.data.permissions)) return
        setPublishOpen(false)
        setRetireOpen(false)
        setCreateOpen(false)
    }, [profileQuery.data])

    const replaceView = (view: typeof urlState.view, version?: string) => {
        const query = buildDetailSearchParams({ view, version })
        router.replace(
            query
                ? `/system/approval-processes/${rawDocumentType}?${query}`
                : `/system/approval-processes/${rawDocumentType}`,
        )
    }

    if (!documentType) {
        return (
            <PageScaffold>
                <PageHeader title="审批流程配置" />
                <BusinessFailureState
                    kind="validation"
                    title="单据类型无效"
                    description="地址中的单据类型不在固定目录中。"
                    action={
                        <Button
                            type="button"
                            render={<Link href="/system/approval-processes" />}
                        >
                            返回目录
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (unknownParams) {
        return (
            <PageScaffold>
                <PageHeader title="审批流程配置" />
                <BusinessFailureState
                    kind="validation"
                    title="查询条件无效"
                    description="地址中的查看参数无法识别。"
                    action={
                        <Button
                            type="button"
                            onClick={() =>
                                router.replace(
                                    `/system/approval-processes/${documentType}`,
                                )
                            }
                        >
                            清除查看条件
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (profileQuery.data && !canReadCatalog(profileQuery.data.permissions)) {
        return (
            <PageScaffold>
                <PageHeader title="审批流程配置" />
                <BusinessFailureState
                    kind="permission"
                    title="权限不足"
                    description="当前账号不能查看审批流程定义。"
                    action={
                        <Button
                            type="button"
                            render={<Link href="/system/approval-processes" />}
                        >
                            返回目录
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (catalogItem?.approval_requirement === "NO_APPROVAL") {
        return (
            <PageScaffold>
                <PageHeader
                    title={documentTypeLabel(
                        documentType,
                        catalogItem.document_type_label,
                    )}
                    description="无需审批 / 不适用"
                    actions={
                        <Button
                            type="button"
                            variant="outline"
                            render={<Link href="/system/approval-processes" />}
                        >
                            返回目录
                        </Button>
                    }
                />
                <p className="text-sm text-muted-foreground">
                    该单据类型不配置审批流程，也没有新建、编辑、发布或退役入口。
                </p>
            </PageScaffold>
        )
    }

    const permissions = profileQuery.data?.permissions
    const canCreate =
        catalogItem != null &&
        canPerformCatalogAction("CREATE_DRAFT", catalogItem, permissions)
    const canPublish =
        catalogItem != null &&
        canPerformCatalogAction("PUBLISH", catalogItem, permissions)
    const canRetire =
        catalogItem != null &&
        canPerformCatalogAction("RETIRE", catalogItem, permissions)
    const missing =
        catalogItem?.configuration_status === "MISSING_CONFIGURATION"

    return (
        <PageScaffold density="compact">
            <PageHeader
                title={documentTypeLabel(
                    documentType,
                    catalogItem?.document_type_label,
                )}
                description={
                    catalogItem
                        ? configurationStatusLabel(
                              catalogItem.configuration_status,
                              catalogItem.approval_requirement,
                          )
                        : "审批流程"
                }
                actions={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            variant="outline"
                            render={<Link href="/system/approval-processes" />}
                        >
                            返回目录
                        </Button>
                        <Button
                            type="button"
                            variant={
                                urlState.view === "current"
                                    ? "default"
                                    : "outline"
                            }
                            onClick={() => replaceView("current")}
                        >
                            当前版本
                        </Button>
                        <Button
                            type="button"
                            variant={
                                urlState.view === "draft"
                                    ? "default"
                                    : "outline"
                            }
                            onClick={() => replaceView("draft")}
                        >
                            草稿
                        </Button>
                        <Button
                            type="button"
                            variant={
                                urlState.view === "history"
                                    ? "default"
                                    : "outline"
                            }
                            onClick={() => replaceView("history")}
                        >
                            历史版本
                        </Button>
                        {canCreate ? (
                            <Button
                                type="button"
                                onClick={() => setCreateOpen(true)}
                            >
                                新建草稿
                            </Button>
                        ) : null}
                        {canPublish && draft && urlState.view === "draft" ? (
                            <Button
                                type="button"
                                onClick={() => setPublishOpen(true)}
                            >
                                发布
                            </Button>
                        ) : null}
                        {canRetire && published ? (
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() => setRetireOpen(true)}
                            >
                                退役
                            </Button>
                        ) : null}
                    </div>
                }
            />

            {missing && !draft ? (
                <BusinessFailureState
                    kind="business"
                    title="配置缺失"
                    description="该单据类型必须审批，但还没有可绑定的已发布流程。创建新单据会被阻断。"
                    action={
                        canCreate ? (
                            <Button
                                type="button"
                                onClick={() => setCreateOpen(true)}
                            >
                                新建草稿
                            </Button>
                        ) : null
                    }
                />
            ) : null}

            {urlState.view === "draft" && !draft ? (
                <p className="text-sm text-muted-foreground">
                    当前没有草稿。
                    {canCreate
                        ? "请先创建草稿后再编辑。"
                        : "你没有创建草稿的权限。"}
                </p>
            ) : null}

            {urlState.view === "history" ? (
                <VersionHistory
                    versions={versionsQuery.data ?? []}
                    selectedVersion={urlState.version}
                    onSelect={(item) =>
                        replaceView("history", item.definition_version)
                    }
                />
            ) : null}

            {detailQuery.isError ? (
                <BusinessFailureState
                    kind="system"
                    title="审批流程加载失败"
                    description={definitionErrorMessage(detailQuery.error)}
                    action={
                        <Button
                            type="button"
                            onClick={() => void detailQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            ) : null}

            {detailQuery.data &&
            (urlState.view !== "history" || historyTarget) &&
            !(urlState.view === "draft" && !draft) ? (
                <DefinitionEditor
                    detail={detailQuery.data}
                    lockVersion={
                        lockVersion || detailQuery.data.definition_lock_version
                    }
                    onLockVersionChange={(next) => {
                        setLockVersion(next)
                        void detailQuery.refetch()
                    }}
                />
            ) : null}

            {catalogItem ? (
                <CreateDraftDialog
                    item={catalogItem}
                    open={createOpen}
                    onOpenChange={setCreateOpen}
                    onCreated={() => replaceView("draft")}
                />
            ) : null}
            {detailQuery.data &&
            draft &&
            detailQuery.data.definition_id === draft.definition_id ? (
                <PublishDialog
                    detail={detailQuery.data}
                    lockVersion={
                        lockVersion || detailQuery.data.definition_lock_version
                    }
                    open={publishOpen}
                    onOpenChange={setPublishOpen}
                    onConflict={() => void detailQuery.refetch()}
                    onPublished={() => replaceView("current")}
                />
            ) : null}
            {published ? (
                <RetireTarget
                    publishedId={published.definition_id}
                    open={retireOpen}
                    onOpenChange={setRetireOpen}
                />
            ) : null}
        </PageScaffold>
    )
}

/**
 * 退役目标必须是当前已发布定义，避免误退役草稿。
 */
function RetireTarget({
    publishedId,
    open,
    onOpenChange,
}: {
    publishedId: string
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const publishedQuery = useDefinitionDetailQuery(publishedId, open)
    if (!publishedQuery.data) return null
    return (
        <RetireDialog
            detail={publishedQuery.data}
            lockVersion={publishedQuery.data.definition_lock_version}
            open={open}
            onOpenChange={onOpenChange}
            onConflict={() => void publishedQuery.refetch()}
            onRetired={() => {
                void publishedQuery.refetch()
            }}
        />
    )
}

export type { DefinitionCatalogItem }
