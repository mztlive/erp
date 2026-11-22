"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    ArchiveIcon,
    EllipsisIcon,
    PencilLineIcon,
    PlusIcon,
} from "lucide-react"

import {
    DiscardConfirmDialog,
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceTabs,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"

import { CreateDraftDialog } from "../components/create-draft-dialog"
import { DefinitionReadView } from "../components/definition-read-view"
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
import { lockVersionText } from "../write-payload"
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
    const versions = versionsQuery.isPlaceholderData
        ? []
        : (versionsQuery.data ?? [])
    const published = versions.find((item) => item.status === "PUBLISHED")
    const draft = versions.find((item) => item.status === "DRAFT")
    const historyTarget =
        urlState.view === "history"
            ? urlState.version
                ? versions.find(
                      (item) =>
                          String(item.definition_version) === urlState.version,
                  )
                : (published ?? versions[0])
            : undefined
    const targetId =
        urlState.view === "draft"
            ? (draft?.definition_id ?? null)
            : urlState.view === "history"
              ? (historyTarget?.definition_id ?? null)
              : (published?.definition_id ?? null)
    const detailQuery = useDefinitionDetailQuery(targetId)
    const [lockVersion, setLockVersion] = React.useState("")
    const [editorDirty, setEditorDirty] = React.useState(false)
    const [pendingNavigation, setPendingNavigation] = React.useState<
        string | null
    >(null)
    const [publishOpen, setPublishOpen] = React.useState(false)
    const [retireOpen, setRetireOpen] = React.useState(false)
    const [createOpen, setCreateOpen] = React.useState(false)

    React.useEffect(() => {
        if (detailQuery.data) {
            setLockVersion(
                lockVersionText(detailQuery.data.definition_lock_version),
            )
        }
    }, [detailQuery.data])

    React.useEffect(() => {
        if (!profileQuery.data) return
        if (canReadCatalog(profileQuery.data.permissions)) return
        setPublishOpen(false)
        setRetireOpen(false)
        setCreateOpen(false)
    }, [profileQuery.data])

    const navigate = (href: string) => {
        if (editorDirty) setPendingNavigation(href)
        else router.replace(href)
    }

    const replaceView = (view: typeof urlState.view, version?: string) => {
        const query = buildDetailSearchParams({ view, version })
        router.replace(`/system/approval-processes/${rawDocumentType}${query}`)
    }

    const typeTitle = documentType
        ? documentTypeLabel(documentType, catalogItem?.document_type_label)
        : "审批流程配置"

    if (!documentType) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="审批流程配置" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            kind="validation"
                            title="单据类型无效"
                            description="地址中的单据类型不在固定目录中。"
                            action={
                                <Button
                                    id="governance-approval-processes-detail-invalid-back"
                                    type="button"
                                    render={
                                        <Link href="/system/approval-processes" />
                                    }
                                >
                                    返回目录
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (unknownParams) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="审批流程配置" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            kind="validation"
                            title="查询条件无效"
                            description="地址中的查看参数无法识别。"
                            action={
                                <Button
                                    id="governance-approval-processes-detail-unknown-clear"
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
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (profileQuery.data && !canReadCatalog(profileQuery.data.permissions)) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="审批流程配置" />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            kind="permission"
                            title="权限不足"
                            description="当前账号不能查看审批流程定义。"
                            action={
                                <Button
                                    id="governance-approval-processes-detail-permission-back"
                                    type="button"
                                    render={
                                        <Link href="/system/approval-processes" />
                                    }
                                >
                                    返回目录
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (catalogItem?.approval_requirement === "NO_APPROVAL") {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title={typeTitle}
                    meta="无需审批 / 不适用"
                    secondaryActions={
                        <Button
                            id="governance-approval-processes-detail-no-approval-back"
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.quietButton}
                            render={<Link href="/system/approval-processes" />}
                        >
                            返回目录
                        </Button>
                    }
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <p className="text-body-compact text-muted-foreground">
                            该单据类型不配置审批流程，也没有新建、编辑、发布或停用入口。
                        </p>
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const permissions = profileQuery.data?.permissions
    const canCreate =
        catalogItem != null &&
        canPerformCatalogAction("CREATE_DRAFT", catalogItem, permissions)
    const canEdit =
        catalogItem != null &&
        canPerformCatalogAction("REPLACE_NODES", catalogItem, permissions)
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
        !detailQuery.isPlaceholderData &&
        !detailQuery.isError &&
        (urlState.view !== "history" || Boolean(historyTarget)) &&
        !(urlState.view === "draft" && !draft)
    const editingDraft =
        showEditor &&
        detailQuery.data?.status === "DRAFT" &&
        canEdit &&
        urlState.view !== "history"
    const publishAction =
        canPublish && draft ? (
            <Button
                id="governance-approval-processes-detail-publish"
                type="button"
                size="sm"
                disabled={editorDirty}
                title={editorDirty ? "请先保存草稿后再发布" : undefined}
                onClick={() => setPublishOpen(true)}
            >
                发布新版本
            </Button>
        ) : null

    const draftAction =
        draft && urlState.view !== "draft" ? (
            <Button
                id="governance-approval-processes-detail-continue-draft"
                type="button"
                size="sm"
                variant={canEdit ? "default" : "ghost"}
                className={
                    canEdit ? undefined : detailWorkspaceStyles.quietButton
                }
                onClick={() =>
                    navigate(
                        `/system/approval-processes/${rawDocumentType}${buildDetailSearchParams({ view: "draft" })}`,
                    )
                }
            >
                <PencilLineIcon aria-hidden data-icon="inline-start" />
                {canEdit ? "继续编辑草稿" : "查看草稿"}
            </Button>
        ) : null

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                back={{
                    id: "governance-approval-processes-detail-back",
                    label: "审批流程目录",
                    onClick: () => navigate("/system/approval-processes"),
                }}
                title={typeTitle}
                titleExtra={
                    editingDraft && editorDirty ? (
                        <DetailWorkspaceDirtyBadge />
                    ) : null
                }
                primaryStatus={
                    detailQuery.data
                        ? {
                              tone: definitionStatusTone(
                                  detailQuery.data.status,
                              ),
                              label: definitionStatusLabel(
                                  detailQuery.data.status,
                              ),
                          }
                        : catalogItem
                          ? {
                                tone: configurationStatusTone(
                                    catalogItem.configuration_status,
                                    catalogItem.approval_requirement,
                                ),
                                label: configurationStatusLabel(
                                    catalogItem.configuration_status,
                                    catalogItem.approval_requirement,
                                ),
                            }
                          : undefined
                }
                meta={
                    <DetailWorkspaceMeta
                        items={[
                            detailQuery.data ? (
                                <span key="version" className="num">
                                    {versionLabel(
                                        detailQuery.data.definition_version,
                                    )}
                                </span>
                            ) : null,
                            catalogItem
                                ? approvalRequirementLabel(
                                      catalogItem.approval_requirement,
                                  )
                                : "审批流程",
                        ]}
                    />
                }
                secondaryActions={
                    <>
                        {canRetire &&
                        published &&
                        urlState.view === "current" ? (
                            <DropdownMenu>
                                <DropdownMenuTrigger
                                    id="governance-approval-processes-detail-more"
                                    render={
                                        <Button
                                            variant="ghost"
                                            size="sm"
                                            className={
                                                detailWorkspaceStyles.quietButton
                                            }
                                        />
                                    }
                                >
                                    <EllipsisIcon
                                        aria-hidden
                                        data-icon="inline-start"
                                    />
                                    更多操作
                                </DropdownMenuTrigger>
                                <DropdownMenuContent align="end">
                                    <DropdownMenuItem
                                        id="governance-approval-processes-detail-retire"
                                        variant="destructive"
                                        onClick={() => setRetireOpen(true)}
                                    >
                                        <ArchiveIcon aria-hidden />
                                        停用当前版本
                                    </DropdownMenuItem>
                                </DropdownMenuContent>
                            </DropdownMenu>
                        ) : null}
                        {!canEdit ? draftAction : null}
                    </>
                }
                primaryAction={
                    <>
                        {canEdit ? draftAction : null}
                        {canCreate && !draft ? (
                            <Button
                                id="governance-approval-processes-detail-create-draft"
                                type="button"
                                size="sm"
                                onClick={() => setCreateOpen(true)}
                            >
                                {published ? (
                                    <PencilLineIcon
                                        aria-hidden
                                        data-icon="inline-start"
                                    />
                                ) : (
                                    <PlusIcon
                                        aria-hidden
                                        data-icon="inline-start"
                                    />
                                )}
                                {published ? "调整流程" : "新建流程"}
                            </Button>
                        ) : null}
                        {urlState.view === "draft" && !editingDraft
                            ? publishAction
                            : null}
                    </>
                }
                tabs={
                    <DetailWorkspaceTabs
                        value={urlState.view}
                        ariaLabel="审批流程版本"
                        tabIdPrefix="governance-approval-processes-detail-tab"
                        onValueChange={(next) => {
                            if (
                                (next === "current" ||
                                    next === "draft" ||
                                    next === "history") &&
                                next !== urlState.view
                            ) {
                                navigate(
                                    `/system/approval-processes/${rawDocumentType}${buildDetailSearchParams({ view: next })}`,
                                )
                            }
                        }}
                        items={[
                            {
                                id: "current",
                                label: "当前版本",
                                badge: published ? (
                                    <span className="num text-xs text-muted-foreground">
                                        v{published.definition_version}
                                    </span>
                                ) : null,
                            },
                            {
                                id: "draft",
                                label: "草稿",
                                badge: draft ? (
                                    <span className="num text-xs text-muted-foreground">
                                        v{draft.definition_version}
                                    </span>
                                ) : null,
                            },
                            { id: "history", label: "历史版本" },
                        ]}
                    />
                }
            />

            <DetailWorkspaceBody>
                {catalogQuery.isError || versionsQuery.isError ? (
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            kind="system"
                            title="流程版本加载失败"
                            description={definitionErrorMessage(
                                catalogQuery.error ?? versionsQuery.error,
                            )}
                            action={
                                <Button
                                    id="governance-approval-processes-versions-retry"
                                    type="button"
                                    variant="outline"
                                    onClick={() => {
                                        if (catalogQuery.isError)
                                            void catalogQuery.refetch()
                                        else void versionsQuery.refetch()
                                    }}
                                >
                                    重试
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                ) : catalogQuery.isPending || versionsQuery.isPending ? (
                    <DetailWorkspaceSection width="form">
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            正在加载流程版本…
                        </p>
                    </DetailWorkspaceSection>
                ) : null}

                {missing &&
                !draft &&
                !versionsQuery.isPending &&
                !versionsQuery.isError &&
                urlState.view === "current" ? (
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            kind="business"
                            title="配置缺失"
                            description="该类单据要求必须经过审批，但当前尚无启用中的审批流程，业务人员将暂时无法新建此类单据。"
                            action={
                                canCreate ? (
                                    <Button
                                        id="governance-approval-processes-detail-missing-create"
                                        type="button"
                                        onClick={() => setCreateOpen(true)}
                                    >
                                        新建草稿
                                    </Button>
                                ) : null
                            }
                        />
                    </DetailWorkspaceSection>
                ) : null}

                {urlState.view === "current" && !published && draft ? (
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="尚未发布审批流程"
                            description="已有一份草稿，发布后用于新建单据。"
                            className="rounded-none border-0 bg-transparent px-0 shadow-none ring-0"
                            action={
                                <Button
                                    id="governance-approval-processes-detail-open-draft"
                                    type="button"
                                    variant="outline"
                                    onClick={() => replaceView("draft")}
                                >
                                    打开草稿
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                ) : null}
                {urlState.view === "history" &&
                urlState.version &&
                !historyTarget &&
                !versionsQuery.isPending ? (
                    <DetailWorkspaceSection width="form">
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            所选版本不存在，请选择其它版本。
                        </p>
                    </DetailWorkspaceSection>
                ) : null}
                {urlState.view === "draft" &&
                !draft &&
                !versionsQuery.isPending &&
                !versionsQuery.isError ? (
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="当前没有草稿"
                            description={
                                canCreate
                                    ? "请先创建草稿后再编辑。"
                                    : "你没有创建草稿的权限。"
                            }
                            className="rounded-none border-0 bg-transparent px-0 shadow-none ring-0"
                            action={
                                canCreate ? (
                                    <Button
                                        id="governance-approval-processes-detail-empty-create"
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
                    </DetailWorkspaceSection>
                ) : null}

                {urlState.view === "history" ? (
                    <DetailWorkspaceSection>
                        <VersionHistory
                            id="governance-approval-processes-detail-history"
                            versions={versionsQuery.data ?? []}
                            selectedVersion={
                                historyTarget
                                    ? String(historyTarget.definition_version)
                                    : undefined
                            }
                            onSelect={(item) =>
                                replaceView(
                                    "history",
                                    String(item.definition_version),
                                )
                            }
                        />
                    </DetailWorkspaceSection>
                ) : null}

                {targetId &&
                detailQuery.isFetching &&
                (detailQuery.isPending || detailQuery.isPlaceholderData) ? (
                    <DetailWorkspaceSection width="form">
                        <p
                            role="status"
                            className="text-body-compact text-muted-foreground"
                        >
                            正在加载审批流程…
                        </p>
                    </DetailWorkspaceSection>
                ) : null}

                {detailQuery.isError ? (
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            kind="system"
                            title="审批流程加载失败"
                            description={definitionErrorMessage(
                                detailQuery.error,
                            )}
                            action={
                                <Button
                                    id="governance-approval-processes-detail-retry"
                                    type="button"
                                    onClick={() => void detailQuery.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                ) : null}

                {showEditor && detailQuery.data ? (
                    editingDraft ? (
                        <DefinitionEditor
                            key={detailQuery.data.definition_id}
                            onDirtyChange={setEditorDirty}
                            publishAction={publishAction}
                            id="governance-approval-processes-detail-editor"
                            detail={detailQuery.data}
                            lockVersion={
                                lockVersion ||
                                lockVersionText(
                                    detailQuery.data.definition_lock_version,
                                )
                            }
                            onLockVersionChange={(next) => {
                                setLockVersion(next)
                                void detailQuery.refetch()
                            }}
                        />
                    ) : (
                        <DefinitionReadView
                            key={detailQuery.data.definition_id}
                            detail={detailQuery.data}
                        />
                    )
                ) : null}
            </DetailWorkspaceBody>

            <DiscardConfirmDialog
                id="governance-approval-processes-discard"
                open={pendingNavigation !== null}
                onOpenChange={(open) => {
                    if (!open) setPendingNavigation(null)
                }}
                onConfirm={() => {
                    if (pendingNavigation) {
                        setEditorDirty(false)
                        router.replace(pendingNavigation)
                        setPendingNavigation(null)
                    }
                }}
            />

            {catalogItem ? (
                <CreateDraftDialog
                    id="governance-approval-processes-detail-create-draft-dialog"
                    item={catalogItem}
                    initialSource={published ? "CURRENT_PUBLISHED" : "EMPTY"}
                    open={createOpen}
                    onOpenChange={setCreateOpen}
                    onCreated={() => replaceView("draft")}
                />
            ) : null}
            {detailQuery.data &&
            draft &&
            detailQuery.data.definition_id === draft.definition_id ? (
                <PublishDialog
                    id="governance-approval-processes-detail-publish-dialog"
                    detail={detailQuery.data}
                    lockVersion={
                        lockVersion ||
                        lockVersionText(
                            detailQuery.data.definition_lock_version,
                        )
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
        </DetailWorkspacePage>
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
            lockVersion={lockVersionText(
                publishedQuery.data.definition_lock_version,
            )}
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
