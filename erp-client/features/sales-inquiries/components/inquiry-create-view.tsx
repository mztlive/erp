"use client"

import * as React from "react"
import { useMutation } from "@tanstack/react-query"
import { RotateCcwIcon, SaveIcon, UploadIcon } from "lucide-react"
import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    DiscardConfirmDialog,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { useRouter } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { inquiryPath, previewInquiryCreation } from "../api"
import { useInquiry } from "../hooks/queries"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { resolveInquiryBackLink } from "../lib/navigation"
import {
    InquiryDemandEditor,
    type InquiryDemandEditorHandle,
    type InquiryDemandEditorStatus,
} from "./inquiry-demand-editor"
import {
    InquiryImportDialog,
    type InquiryImportConfirmResult,
} from "./inquiry-import-dialog"

export function InquiryCreateView({
    copyFromId,
    returnTo,
    userId,
    ownerName,
    permissions,
}: {
    copyFromId: string
    returnTo?: string | null
    userId: string
    ownerName?: string
    permissions: readonly string[]
}) {
    const router = useRouter()
    const editorRef = React.useRef<InquiryDemandEditorHandle>(null)
    const [importOpen, setImportOpen] = React.useState(false)
    const [resetOpen, setResetOpen] = React.useState(false)
    const [editorStatus, setEditorStatus] =
        React.useState<InquiryDemandEditorStatus>({
            dirty: false,
            saving: false,
            disabled: false,
            lineCount: 0,
            pendingDeliveryCount: 0,
            canSave: true,
            customerId: "",
            pricedLineCount: 0,
            referenceTotal: null,
        })

    const title = copyFromId ? "复制为新询价" : "新建销售询价"
    const back = resolveInquiryBackLink({
        id: "sales-inquiry-new-back",
        procurement: false,
        returnTo,
    })
    const copyQuery = useInquiry(
        copyFromId,
        false,
        undefined,
        Boolean(copyFromId),
    )
    const canCreate =
        hasPermission(permissions, "sales_inquiry:create") &&
        hasPermission(permissions, "sellable_sku:list")
    const canImport =
        canCreate &&
        !copyFromId &&
        hasPermission(permissions, "sales_inquiry:import")

    const importCommand = useInquiryCommand(userId, "import-new")
    const importPreview = useMutation({
        mutationFn: ({
            file,
            customerId,
        }: {
            file: File
            customerId: string
        }) => previewInquiryCreation(file, customerId),
        meta: { suppressErrorToast: true },
    })

    React.useEffect(() => {
        const result = importCommand.resolution?.result as
            | InquiryImportConfirmResult
            | undefined
        if (importCommand.resolution?.replayed && result?.inquiry_id) {
            router.replace(
                `/sales/inquiries/${encodeURIComponent(result.inquiry_id)}`,
            )
        }
    }, [importCommand.resolution, router])

    // Cmd/Ctrl + S 快捷创建询价草稿（避开输入法与弹窗）。
    React.useEffect(() => {
        if (!canCreate) return
        const onKeyDown = (event: KeyboardEvent) => {
            if (
                event.defaultPrevented ||
                event.isComposing ||
                event.repeat ||
                event.altKey ||
                event.shiftKey ||
                !(event.metaKey || event.ctrlKey) ||
                event.key.toLowerCase() !== "s" ||
                (event.target instanceof Element &&
                    event.target.closest(
                        '[role="dialog"], [role="alertdialog"]',
                    ))
            ) {
                return
            }
            event.preventDefault()
            if (
                !editorStatus.saving &&
                !editorStatus.disabled &&
                !importOpen &&
                !resetOpen
            ) {
                editorRef.current?.save()
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [
        canCreate,
        editorStatus.disabled,
        editorStatus.saving,
        importOpen,
        resetOpen,
    ])

    if (!canCreate) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title={title} back={back} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <Alert variant="destructive">
                            <AlertDescription>
                                当前账号没有新建询价或选择商品权限。
                            </AlertDescription>
                        </Alert>
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (copyFromId && copyQuery.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title={title}
                description="正在读取原询价…"
            />
        )
    }

    if (copyFromId && copyQuery.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title={title} back={back} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            id="sales-inquiry-copy-retry"
                            title="原询价读取失败"
                            error={copyQuery.error}
                            onRetry={() => void copyQuery.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const copySource =
        copyFromId && copyQuery.data && "sales_owner_user_id" in copyQuery.data
            ? copyQuery.data
            : undefined

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={title}
                back={back}
                primaryStatus={{
                    label: copyFromId ? "复制草稿" : "待创建",
                    tone: "info",
                }}
                titleExtra={
                    editorStatus.dirty ? (
                        <DetailWorkspaceDirtyBadge label="本机已暂存" />
                    ) : null
                }
                meta={
                    <DetailWorkspaceMeta
                        items={[
                            ownerName ? `销售负责：${ownerName}` : null,
                            copySource ? (
                                <span key="copy-source" className="num">
                                    来源单号：{copySource.inquiry_no}
                                </span>
                            ) : null,
                        ]}
                    />
                }
                secondaryActions={
                    <>
                        {canImport && (
                            <Button
                                id="sales-inquiry-create-import-open"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                disabled={editorStatus.saving}
                                onClick={() => setImportOpen(true)}
                            >
                                <UploadIcon
                                    aria-hidden="true"
                                    data-icon="inline-start"
                                />
                                导入客户选品
                            </Button>
                        )}
                        {editorStatus.dirty && (
                            <Button
                                id="sales-inquiry-create-reset"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                disabled={editorStatus.saving}
                                onClick={() => setResetOpen(true)}
                            >
                                <RotateCcwIcon
                                    aria-hidden="true"
                                    data-icon="inline-start"
                                />
                                {copyFromId ? "恢复原询价" : "清空草稿"}
                            </Button>
                        )}
                    </>
                }
                primaryAction={
                    editorStatus.canSave ? (
                        <LoadingButton
                            id="sales-inquiry-save-demand"
                            type="button"
                            size="sm"
                            className="px-3.5"
                            loading={editorStatus.saving}
                            disabled={editorStatus.disabled}
                            onClick={() => editorRef.current?.save()}
                        >
                            <SaveIcon
                                aria-hidden="true"
                                data-icon="inline-start"
                            />
                            {editorStatus.saving ? "正在创建…" : "创建询价草稿"}
                        </LoadingButton>
                    ) : null
                }
            />
            <DetailWorkspaceBody
                alerts={
                    copyFromId ? (
                        <Alert variant="info">
                            <AlertDescription>
                                已带入原询价商品与配送需求；请核对交付日并设置新的询价截止日期，原报价及转单占用不会带入。
                            </AlertDescription>
                        </Alert>
                    ) : undefined
                }
            >
                <InquiryDemandEditor
                    ref={editorRef}
                    copyFrom={copySource}
                    userId={userId}
                    permissions={permissions}
                    externalSaveChrome
                    onImport={canImport ? () => setImportOpen(true) : undefined}
                    onStatusChange={setEditorStatus}
                    onSaved={() => undefined}
                />
            </DetailWorkspaceBody>

            <DiscardConfirmDialog
                id="sales-inquiry-create-reset-confirm"
                open={resetOpen}
                onOpenChange={setResetOpen}
                title={copyFromId ? "恢复为原询价内容？" : "清空当前暂存内容？"}
                description={
                    copyFromId
                        ? "本机暂存的修改将被清除，表单恢复为原询价的商品与配送需求。"
                        : "当前未创建的客户、商品与配送输入将被清空，无法撤销。"
                }
                confirmLabel={copyFromId ? "恢复原询价" : "确认清空"}
                cancelLabel="继续编辑"
                onConfirm={() => {
                    setResetOpen(false)
                    editorRef.current?.reset()
                }}
            />

            {canImport && (
                <InquiryImportDialog
                    creation
                    open={importOpen}
                    onOpenChange={setImportOpen}
                    initialCustomerId={editorStatus.customerId}
                    finalFocusId="sales-inquiry-create-import-open"
                    busy={importPreview.isPending || importCommand.isPending}
                    disabled={!importCommand.ready || importCommand.unresolved}
                    error={importCommand.error ?? importPreview.error}
                    onPreview={(file, customerId) =>
                        importPreview.mutateAsync({ file, customerId })
                    }
                    onConfirm={async (result, confirmMerge, customerId) => {
                        const created =
                            await importCommand.run<InquiryImportConfirmResult>(
                                `${inquiryPath()}/imports/create`,
                                {
                                    customer_id: customerId,
                                    preview_id: result.preview_id,
                                    input_fingerprint: result.input_fingerprint,
                                    confirm_merge_duplicates: confirmMerge,
                                },
                            )
                        router.replace(
                            `/sales/inquiries/${encodeURIComponent(created.inquiry_id)}`,
                        )
                    }}
                    recovery={
                        importCommand.unresolved ? (
                            <Alert variant="warning">
                                <AlertDescription>
                                    上次导入创建结果尚待核对，请先确认，避免重复创建。
                                    <Button
                                        id="sales-inquiry-create-import-recover"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={importCommand.isPending}
                                        onClick={() =>
                                            void importCommand
                                                .recover()
                                                .catch(() => undefined)
                                        }
                                    >
                                        核对上次创建结果
                                    </Button>
                                </AlertDescription>
                            </Alert>
                        ) : undefined
                    }
                />
            )}
        </DetailWorkspacePage>
    )
}
