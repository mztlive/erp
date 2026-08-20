"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { cn } from "@/lib/utils"

import { CreateDraftDialog } from "../components/create-draft-dialog"
import { DefinitionEditor } from "../components/definition-editor"
import { PublishDialog } from "../components/publish-dialog"
import { RetireDialog } from "../components/retire-dialog"
import { VersionHistory } from "../components/version-history"
import { definitionErrorMessage } from "../errors"
import {
    approvalRequirementLabel,
    configurationStatusLabel,
    configurationStatusTone,
    definitionStatusLabel,
    definitionStatusTone,
    documentTypeLabel,
    versionLabel,
} from "../labels"
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
        router.replace(`/system/approval-processes/${rawDocumentType}${query}`)
    }

    const typeTitle = documentType
        ? documentTypeLabel(documentType, catalogItem?.document_type_label)
        : "审批流程配置"
    const catalogBreadcrumbs = [
        { id: "system", label: "系统", href: "/system/approval-processes" },
        {
            id: "catalog",
            label: "审批流程配置",
            href: "/system/approval-processes",
        },
        { id: "type", label: typeTitle, current: true as const },
    ]

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
                    title={typeTitle}
                    description="无需审批 / 不适用"
                    breadcrumbs={catalogBreadcrumbs}
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
    const showEditor =
        Boolean(detailQuery.data) &&
        (urlState.view !== "history" || Boolean(historyTarget)) &&
        !(urlState.view === "draft" && !draft)

    return (
        <PageScaffold density="compact">
            <PageHeader
                variant="object-chrome"
                breadcrumbs={catalogBreadcrumbs}
                actions={
                    <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        render={<Link href="/system/approval-processes" />}
                    >
                        返回目录
                    </Button>
                }
            />

            <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                <div className="min-w-0 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                        <h1 className="text-xl font-semibold tracking-tight">
                            {typeTitle}
                        </h1>
                        {catalogItem ? (
                            <StatusBadge
                                tone={configurationStatusTone(
                                    catalogItem.configuration_status,
                                    catalogItem.approval_requirement,
                                )}
                                label={configurationStatusLabel(
                                    catalogItem.configuration_status,
                                    catalogItem.approval_requirement,
                                )}
                            />
                        ) : null}
                    </div>
                    <p className="text-sm text-muted-foreground">
                        {catalogItem
                            ? approvalRequirementLabel(
                                  catalogItem.approval_requirement,
                              )
                            : "审批流程"}
                        {" · "}
                        已发布{" "}
                        {published
                            ? versionLabel(published.definition_version)
                            : "—"}
                        {" · "}
                        草稿{" "}
                        {draft ? versionLabel(draft.definition_version) : "—"}
                    </p>
                </div>
                <div className="flex flex-wrap gap-2">
                    {canCreate ? (
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => setCreateOpen(true)}
                        >
                            新建草稿
                        </Button>
                    ) : null}
                    {canPublish && draft && urlState.view === "draft" ? (
                        <Button
                            type="button"
                            size="sm"
                            onClick={() => setPublishOpen(true)}
                        >
                            发布
                        </Button>
                    ) : null}
                    {canRetire && published ? (
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => setRetireOpen(true)}
                        >
                            退役
                        </Button>
                    ) : null}
                </div>
            </div>

            <div
                className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            >
                <Tabs
                    value={urlState.view}
                    onValueChange={(next) => {
                        if (
                            next === "current" ||
                            next === "draft" ||
                            next === "history"
                        ) {
                            replaceView(next)
                        }
                    }}
                >
                    <TabsList
                        variant="line"
                        className="h-auto w-full flex-wrap justify-start gap-1 rounded-none border-b border-grid bg-card px-3 py-1.5"
                    >
                        <TabsTrigger value="current" className="flex-none">
                            当前版本
                        </TabsTrigger>
                        <TabsTrigger value="draft" className="flex-none">
                            草稿
                        </TabsTrigger>
                        <TabsTrigger value="history" className="flex-none">
                            历史版本
                        </TabsTrigger>
                        {detailQuery.data ? (
                            <div className="ml-auto flex items-center gap-2 py-0.5">
                                <StatusBadge
                                    tone={definitionStatusTone(
                                        detailQuery.data.status,
                                    )}
                                    label={definitionStatusLabel(
                                        detailQuery.data.status,
                                    )}
                                />
                                <span className="text-xs text-muted-foreground">
                                    {versionLabel(
                                        detailQuery.data.definition_version,
                                    )}
                                </span>
                            </div>
                        ) : null}
                    </TabsList>
                </Tabs>

                {missing && !draft ? (
                    <div className="p-4">
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
                    </div>
                ) : null}

                {urlState.view === "draft" && !draft ? (
                    <BusinessEmptyState
                        kind="no-data"
                        title="当前没有草稿"
                        description={
                            canCreate
                                ? "请先创建草稿后再编辑。"
                                : "你没有创建草稿的权限。"
                        }
                        className="rounded-none border-0 bg-transparent p-6 shadow-none ring-0"
                        action={
                            canCreate ? (
                                <Button
                                    type="button"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    onClick={() => setCreateOpen(true)}
                                >
                                    新建草稿
                                </Button>
                            ) : null
                        }
                    />
                ) : null}

                {urlState.view === "history" ? (
                    <div
                        className={
                            showEditor ? "border-b border-grid" : undefined
                        }
                    >
                        <VersionHistory
                            versions={versionsQuery.data ?? []}
                            selectedVersion={urlState.version}
                            onSelect={(item) =>
                                replaceView("history", item.definition_version)
                            }
                        />
                    </div>
                ) : null}

                {detailQuery.isError ? (
                    <div className="p-4">
                        <BusinessFailureState
                            kind="system"
                            title="审批流程加载失败"
                            description={definitionErrorMessage(
                                detailQuery.error,
                            )}
                            action={
                                <Button
                                    type="button"
                                    onClick={() => void detailQuery.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    </div>
                ) : null}

                {showEditor && detailQuery.data ? (
                    <DefinitionEditor
                        detail={detailQuery.data}
                        lockVersion={
                            lockVersion ||
                            detailQuery.data.definition_lock_version
                        }
                        onLockVersionChange={(next) => {
                            setLockVersion(next)
                            void detailQuery.refetch()
                        }}
                    />
                ) : null}
            </div>

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
