"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"
import { useIsMutating, useMutation } from "@tanstack/react-query"
import { UploadIcon } from "lucide-react"
import {
    BusinessFailureState,
    DetailPageHeader,
    PageScaffold,
    QuickPreviewSheet,
} from "@/components/business"
import { Button, buttonVariants } from "@/components/ui/button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { apiPost, apiPostForm } from "@/lib/api"
import { hasPermission } from "@/lib/permissions"
import { inquiryPath } from "../api"
import { useInquiry } from "../hooks/queries"
import {
    useInquiryCommand,
    useInquiryCommandResolution,
    type InquiryCommandResolution,
} from "../hooks/use-inquiry-command"
import { isWorkspaceFrame, workspaceHost } from "@/lib/workspace-tabs"
import { inquiryTaskCompletion } from "../lib/task-completion"
import { inquiryDate } from "../lib/format"
import { inquiryStorageKey } from "../lib/local-draft"
import {
    downloadInquiryCustomerWorkbook,
    type InquiryCustomerWorkbookProduct,
} from "../lib/customer-workbook"
import { SalesInquiryProducts } from "../components/sales-inquiry-products"
import { salesInquiryFeedback } from "../lib/feedback-view"
import { InquiryDetailTabs } from "../components/inquiry-detail-tabs"
import { InquiryReinquiryNotice } from "../components/inquiry-reinquiry-notice"
import { InquiryFeedbackResults } from "../components/inquiry-feedback-results"
import { InquiryDemandEditor } from "../components/inquiry-demand-editor"
import {
    InquiryFeedbackEditor,
    type InquiryFeedbackEditorHandle,
} from "../components/inquiry-feedback-editor"
import { InquiryActions } from "../components/inquiry-actions"
import { InquiryInternalQuotes } from "../components/inquiry-internal-quotes"
import { InquiryFees } from "../components/inquiry-fees"
import { InquiryRequirementTable } from "../components/inquiry-requirement-table"
import { SupplierQuoteImportButton } from "../components/supplier-quote-import-dialog"
import { SupplierQuoteExportButton } from "../components/supplier-quote-export-dialog"
import {
    InquiryImportDialog,
    InquiryCustomerTemplateButton,
    type InquiryImportPreview,
} from "../components/inquiry-import-dialog"
import {
    INQUIRY_STATUS_LABEL,
    type ProcurementInquiry,
    type SalesInquiry,
} from "../types"

export function SalesInquiryDetailPage({
    inquiryId = "",
    procurement = false,
    workItemId,
    embedded = false,
    onTaskCompleted,
}: {
    inquiryId?: string
    procurement?: boolean
    workItemId?: string
    embedded?: boolean
    onTaskCompleted?: () => void
}) {
    const search = useSearchParams()
    const router = useRouter()
    const mutating = useIsMutating()
    const returnedTask = React.useRef<string | null>(null)
    const taskId =
        workItemId ??
        search.get("task_id") ??
        search.get("workItemId") ??
        undefined
    const copyFromId = !inquiryId ? (search.get("copyFrom") ?? "") : ""
    const profile = useAccountProfileQuery()
    const taskKey = `${inquiryId}:${taskId ?? ""}`
    const [completion, setCompletion] = React.useState<{
        key: string
        message: string
    } | null>(null)
    const completedTask = React.useRef<string | null>(null)
    const onResolved = React.useCallback(
        (resolution: InquiryCommandResolution) => {
            if (!procurement) return
            const result = inquiryTaskCompletion(inquiryId, resolution)
            if (!result || result.taskId !== taskId) return
            // 同步阻止提交回调中的手动刷新，不依赖 React 提交状态更新的时机。
            completedTask.current = taskKey
            setCompletion({ key: taskKey, message: result.message })
        },
        [inquiryId, procurement, taskId, taskKey],
    )
    useInquiryCommandResolution(
        profile.data?.userid ?? "",
        inquiryId,
        onResolved,
    )
    const taskFinished = procurement && completion?.key === taskKey
    const query = useInquiry(
        inquiryId || copyFromId,
        procurement,
        taskId,
        !taskFinished,
    )
    React.useEffect(() => {
        if (!taskFinished || mutating > 0 || returnedTask.current === taskKey)
            return
        // 等待表单卸载和成功命令刷新完成，再让工作区按已保存状态关闭页面。
        const frame = requestAnimationFrame(() => {
            returnedTask.current = taskKey
            if (embedded && onTaskCompleted) {
                onTaskCompleted()
                return
            }
            const host = workspaceHost()
            if (host && isWorkspaceFrame()) {
                window.dispatchEvent(new Event("erp-workspace-form-state"))
                host.changed(window)
                host.closeCurrent(window, "/procurement/inquiries")
            } else {
                router.replace("/procurement/inquiries")
            }
        })
        return () => cancelAnimationFrame(frame)
    }, [embedded, mutating, onTaskCompleted, router, taskFinished, taskKey])
    if (!profile.data)
        return (
            <p className="p-6" role="status">
                正在核对账号权限…
            </p>
        )
    if (!inquiryId && copyFromId && !query.data)
        return (
            <PageScaffold>
                {query.isError ? (
                    <BusinessFailureState
                        id="sales-inquiry-copy-retry"
                        title="原询价读取失败"
                        error={query.error}
                        onRetry={() => void query.refetch()}
                    />
                ) : (
                    <p role="status">正在读取原询价…</p>
                )}
            </PageScaffold>
        )
    if (
        !inquiryId &&
        (!hasPermission(profile.data.permissions, "sales_inquiry:create") ||
            !hasPermission(profile.data.permissions, "sellable_sku:list"))
    )
        return (
            <PageScaffold>
                <p>当前账号没有新建询价或选择商品权限。</p>
            </PageScaffold>
        )
    if (!inquiryId)
        return (
            <PageScaffold className="mx-auto max-w-7xl gap-6">
                <DetailPageHeader
                    title={copyFromId ? "复制为新询价" : "新建销售询价"}
                    back={{
                        id: "sales-inquiry-new-back",
                        href: "/sales/inquiries",
                        label: "返回询价列表",
                    }}
                />
                {copyFromId && (
                    <Alert variant="info">
                        <AlertDescription>
                            从原询价复制需求；请核对交付日期并填写新的询价截止日期。原报价、接受结果和转单占用不会带入。
                        </AlertDescription>
                    </Alert>
                )}
                <InquiryDemandEditor
                    copyFrom={
                        copyFromId &&
                        query.data &&
                        "sales_owner_user_id" in query.data
                            ? query.data
                            : undefined
                    }
                    userId={profile.data.userid}
                    permissions={profile.data.permissions}
                    onSaved={() => undefined}
                />
            </PageScaffold>
        )
    if (taskFinished) return null
    if (query.isError)
        return (
            <div className="space-y-4 p-6">
                <BusinessFailureState
                    id="sales-inquiry-detail-retry"
                    title="询价资料加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
                <Link
                    id="sales-inquiry-error-back"
                    href={
                        procurement
                            ? "/procurement/inquiries"
                            : "/sales/inquiries"
                    }
                    className={buttonVariants({ variant: "outline" })}
                >
                    返回询价列表
                </Link>
            </div>
        )
    if (!query.data)
        return (
            <p className="p-6" role="status">
                正在读取询价资料…
            </p>
        )
    return (
        <InquiryDetailContent
            key={`${procurement}:${inquiryId}:${taskId ?? "sales"}`}
            inquiry={query.data}
            procurement={procurement}
            taskId={taskId}
            embedded={embedded}
            userId={profile.data.userid}
            permissions={profile.data.permissions}
            onSaved={() => {
                if (completedTask.current !== taskKey) void query.refetch()
            }}
        />
    )
}

function InquiryDetailContent({
    inquiry,
    procurement,
    taskId,
    embedded,
    userId,
    permissions,
    onSaved,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    procurement: boolean
    taskId?: string
    embedded: boolean
    userId: string
    permissions: readonly string[]
    onSaved: () => void
}) {
    const [importOpen, setImportOpen] = React.useState(false)
    const [dirty, setDirty] = React.useState(false)
    const [revising, setRevising] = React.useState(false)
    const [feedbackDirty, setFeedbackDirty] = React.useState(false)
    const [publishing, setPublishing] = React.useState(false)
    const feedbackEditor = React.useRef<InquiryFeedbackEditorHandle>(null)
    const [quoteLineId, setQuoteLineId] = React.useState<string | null>(null)
    const [feedbackFooterContainer, setFeedbackFooterContainer] =
        React.useState<HTMLDivElement | null>(null)
    const [feedbackVisible, setFeedbackVisible] = React.useState(false)
    const [historyActionsContainer, setHistoryActionsContainer] =
        React.useState<HTMLDivElement | null>(null)
    const [editorEpoch, setEditorEpoch] = React.useState(0)
    const [demandDetailsContainer, setDemandDetailsContainer] =
        React.useState<HTMLDivElement | null>(null)
    const [demandActionsContainer, setDemandActionsContainer] =
        React.useState<HTMLDivElement | null>(null)
    const roundContext = `${inquiry.requirement_version}:${inquiry.feedback_round}:${inquiry.status}`
    const [roundSelection, setRoundSelection] = React.useState<{
        context: string
        round: number | null
    } | null>(null)
    const chosenRound =
        roundSelection?.context === roundContext ? roundSelection.round : null
    const setChosenRound = (round: number | null) =>
        setRoundSelection({ context: roundContext, round })
    const selectedFeedback =
        inquiry.feedbacks.find((item) => item.round === chosenRound) ??
        [...inquiry.feedbacks].sort(
            (left, right) => right.round - left.round,
        )[0]
    const selectedRound = selectedFeedback?.round ?? inquiry.feedback_round
    const productFeedback =
        "sales_owner_user_id" in inquiry
            ? salesInquiryFeedback(inquiry, chosenRound)
            : selectedFeedback
    const actionRound = productFeedback?.round ?? inquiry.feedback_round
    const demandActions = ["draft", "awaiting_sales_supplement"].includes(
        inquiry.status,
    )
    const command = useInquiryCommand(userId, inquiry.id)
    const task =
        "work_items" in inquiry
            ? (inquiry.work_items.find(
                  (item) => item.id === taskId && item.status === "OPEN",
              ) ?? inquiry.work_items.find((item) => item.status === "OPEN"))
            : undefined
    const canEdit =
        !procurement &&
        ["draft", "awaiting_sales_supplement"].includes(inquiry.status) &&
        hasPermission(permissions, "sales_inquiry:update") &&
        hasPermission(permissions, "sellable_sku:list")
    const canRevise =
        !procurement &&
        ["awaiting_sales", "confirmed"].includes(inquiry.status) &&
        hasPermission(permissions, "sales_inquiry:reinquire") &&
        hasPermission(permissions, "sellable_sku:list")
    useInquiryCommandResolution(
        userId,
        inquiry.id,
        React.useCallback(
            (resolution) => {
                const data = resolution.body.data as
                    | { requirement?: unknown }
                    | undefined
                if (
                    resolution.path !==
                        `${inquiryPath(false, inquiry.id)}/reinquire` ||
                    !data?.requirement
                )
                    return
                // 包括退出编辑后恢复的原命令；成功后不再恢复已提交的本地需求。
                try {
                    localStorage.removeItem(
                        inquiryStorageKey(
                            userId,
                            `${inquiry.id}:reinquiry-demand`,
                        ),
                    )
                } catch {
                    // 业务已成功，残留本地输入仍由原版本校验阻止重复提交。
                }
            },
            [inquiry.id, userId],
        ),
    )
    const canImport =
        canEdit && hasPermission(permissions, "sales_inquiry:import")
    const canProcess =
        procurement &&
        task &&
        ["awaiting_procurement", "procurement_processing"].includes(
            inquiry.status,
        ) &&
        hasPermission(permissions, "procurement_inquiry:save_feedback")
    const showReadonlyDemand =
        procurement &&
        !canProcess &&
        !inquiry.feedbacks.some(
            (feedback) =>
                feedback.requirement_version === inquiry.requirement_version,
        )
    const accepted =
        "acceptances" in inquiry ? inquiry.acceptances.at(-1) : undefined
    const canOpenSales =
        !procurement &&
        "sales_progress" in inquiry &&
        inquiry.sales_progress.can_create_sales &&
        accepted != null &&
        hasPermission(permissions, "sales_inquiry:convert_sales") &&
        hasPermission(permissions, "sales_order:create")
    const feeOwner = inquiry.shared_fee_owner_user_id ? (
        <p className="text-sm text-muted-foreground">
            整单费用责任：
            {inquiry.shared_fee_owner_name ||
                (inquiry.shared_fee_owner_user_id === userId
                    ? "由我维护"
                    : "已指定责任采购")}
        </p>
    ) : null
    const safeTemplate = useMutation({
        mutationFn: () =>
            apiPost<InquiryCustomerWorkbookProduct[]>(
                `${inquiryPath()}/template-products`,
                {
                    sku_ids: [
                        ...new Set(
                            inquiry.draft.lines.map((line) => line.sku_id),
                        ),
                    ],
                },
            ),
    })
    const preview = useMutation({
        mutationFn: (file: File) => {
            const data = new FormData()
            data.append("file", file)
            data.append("expected_version", String(inquiry.version))
            return apiPostForm<InquiryImportPreview>(
                `${inquiryPath(false, inquiry.id)}/imports/preview`,
                data,
            )
        },
        meta: { suppressErrorToast: true },
    })
    const productActions = (
        <>
            {!procurement &&
                inquiry.draft.lines.length > 0 &&
                hasPermission(permissions, "sales_inquiry:create") &&
                hasPermission(permissions, "sellable_sku:list") && (
                    <InquiryCustomerTemplateButton
                        id="sales-inquiry-download-template"
                        products={inquiry.draft.lines.map((line) => ({
                            sku_code: line.sku_code,
                            product_name: line.product_name,
                            specification: line.specification,
                            unit: line.unit_code,
                        }))}
                        onLoadProducts={() => safeTemplate.mutateAsync()}
                        inquiry_no={inquiry.inquiry_no}
                        disabled={inquiry.status === "closed"}
                    />
                )}
            {canImport && (
                <Button
                    id="sales-inquiry-detail-import"
                    variant="outline"
                    type="button"
                    onClick={() => setImportOpen(true)}
                >
                    <UploadIcon aria-hidden="true" data-icon="inline-start" />
                    导入客户选品文件
                </Button>
            )}
        </>
    )
    const renderDemand = (primaryActions: React.ReactNode) => (
        <div className="space-y-6">
            {canEdit && "sales_owner_user_id" in inquiry ? (
                <InquiryDemandEditor
                    key={`editor-${editorEpoch}`}
                    inquiry={inquiry}
                    userId={userId}
                    permissions={permissions}
                    onSaved={onSaved}
                    onDirtyChange={setDirty}
                    productActions={productActions}
                    detailsContainer={demandDetailsContainer}
                    headerActionsContainer={demandActionsContainer}
                    footerActions={demandActions ? primaryActions : undefined}
                />
            ) : (
                <InquiryRequirementTable
                    idPrefix="sales-inquiry-requirement"
                    requirement={inquiry.draft}
                    procurement={procurement}
                />
            )}

            {!canEdit &&
                demandActions &&
                React.Children.count(primaryActions) > 0 && (
                    <div className="sticky bottom-0 flex flex-wrap justify-end gap-3 border-t bg-card py-4">
                        {primaryActions}
                    </div>
                )}
        </div>
    )
    const renderSalesActions = (primaryActions: React.ReactNode) => (
        <>
            {!demandActions &&
                (React.Children.count(primaryActions) > 0 || canOpenSales) && (
                    <div className="flex flex-wrap items-center justify-end gap-3">
                        {actionRound !== inquiry.feedback_round && (
                            <p className="mr-auto text-sm text-muted-foreground">
                                正在查看历史轮次，请切回最新一轮后接受或重询。
                            </p>
                        )}
                        {canRevise && (
                            <Button
                                id="sales-inquiry-modify-demand"
                                variant="outline"
                                disabled={
                                    dirty ||
                                    actionRound !== inquiry.feedback_round ||
                                    !command.ready ||
                                    command.isPending ||
                                    command.unresolved
                                }
                                title={
                                    actionRound !== inquiry.feedback_round
                                        ? "请切回最新一轮反馈后修改需求"
                                        : undefined
                                }
                                onClick={() => setRevising(true)}
                            >
                                修改需求
                            </Button>
                        )}
                        {primaryActions}
                        {canOpenSales && (
                            <Link
                                id="sales-inquiry-create-sale"
                                href={`/sales/orders?mode=create&inquiryId=${encodeURIComponent(inquiry.id)}&round=${accepted.feedback_round}`}
                                className={buttonVariants({
                                    size: "sm",
                                })}
                            >
                                按第 {accepted.feedback_round} 轮已接受结果开单
                            </Link>
                        )}
                    </div>
                )}
        </>
    )
    const body = (
        <InquiryActions
            inquiry={inquiry}
            procurement={procurement}
            task={task}
            userId={userId}
            permissions={permissions}
            onSaved={onSaved}
            disabled={dirty || revising}
            feedbackDirty={feedbackDirty}
            beforePublish={
                canProcess
                    ? async () => {
                          if (!feedbackEditor.current)
                              throw new Error(
                                  "采购反馈正在恢复，请稍候再发布。",
                              )
                          return feedbackEditor.current.saveForPublish()
                      }
                    : undefined
            }
            onPublishingChange={setPublishing}
            disabledReason={revising ? "请先提交或取消需求修改" : undefined}
            feedbackRound={actionRound}
            renderActions={(
                primaryActions,
                secondaryActions,
                assignmentActions,
            ) => (
                <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-5 overflow-hidden">
                    <div className="shrink-0 space-y-5">
                        <DetailPageHeader
                            title={inquiry.customer_name}
                            documentNumber={inquiry.inquiry_no}
                            numberLabel="询价单号"
                            version={`需求第 ${inquiry.requirement_version} 版 · 反馈第 ${inquiry.feedback_round} 轮`}
                            primaryStatus={{
                                label: INQUIRY_STATUS_LABEL[inquiry.status],
                                tone:
                                    inquiry.status === "confirmed"
                                        ? "success"
                                        : inquiry.status === "closed"
                                          ? "neutral"
                                          : "info",
                            }}
                            back={
                                embedded
                                    ? undefined
                                    : {
                                          id: "sales-inquiry-detail-back",
                                          href: procurement
                                              ? "/procurement/inquiries"
                                              : "/sales/inquiries",
                                          label: "返回询价列表",
                                      }
                            }
                            secondaryActions={
                                <>
                                    <div
                                        ref={setHistoryActionsContainer}
                                        className="flex items-center"
                                    />
                                    {assignmentActions}
                                    {canEdit && (
                                        <div ref={setDemandActionsContainer} />
                                    )}
                                    {secondaryActions}
                                </>
                            }
                            primaryAction={
                                !procurement &&
                                inquiry.status === "closed" &&
                                hasPermission(
                                    permissions,
                                    "sales_inquiry:create",
                                ) &&
                                hasPermission(
                                    permissions,
                                    "sellable_sku:list",
                                ) ? (
                                    <Link
                                        id="sales-inquiry-copy-new"
                                        href={`/sales/inquiries/new?copyFrom=${encodeURIComponent(inquiry.id)}`}
                                        className={buttonVariants({
                                            size: "sm",
                                        })}
                                    >
                                        复制为新询价
                                    </Link>
                                ) : undefined
                            }
                            meta={
                                canEdit ? undefined : (
                                    <span>
                                        询价截止：
                                        {inquiryDate(inquiry.deadline_at)}
                                    </span>
                                )
                            }
                            summary={
                                canEdit ? (
                                    <div ref={setDemandDetailsContainer} />
                                ) : (
                                    <div className="flex flex-wrap items-start gap-x-8 gap-y-2 rounded-lg bg-muted/40 px-4 py-3">
                                        <p className="flex min-w-0 gap-2 text-sm text-muted-foreground">
                                            <span className="shrink-0">
                                                {productFeedback &&
                                                productFeedback.requirement_version !==
                                                    inquiry.requirement_version
                                                    ? "当前需求说明"
                                                    : "需求说明"}
                                            </span>
                                            <span className="min-w-0 whitespace-pre-wrap wrap-anywhere">
                                                {inquiry.draft.note || "未填写"}
                                            </span>
                                        </p>
                                        {feeOwner}
                                    </div>
                                )
                            }
                        />
                        {procurement &&
                            !task &&
                            inquiry.feedbacks.length > 0 && (
                                <Alert variant="info">
                                    <AlertDescription>
                                        已处理：当前只读展示本次任务发布的反馈及对应需求，后续重询内容以新任务为准。
                                    </AlertDescription>
                                </Alert>
                            )}
                        {canOpenSales && inquiry.status !== "confirmed" && (
                            <Alert variant="info">
                                <AlertDescription>
                                    {inquiry.feedback_round >
                                    accepted.feedback_round
                                        ? `第 ${inquiry.feedback_round} 轮新反馈尚未接受。`
                                        : "当前正在重新询价。"}
                                    开单沿用第 {accepted.feedback_round}{" "}
                                    轮已接受结果，仅列出仍有效、未受本次重询影响且有剩余数量的商品与地址。
                                </AlertDescription>
                            </Alert>
                        )}
                        {!procurement &&
                            "copied_from_inquiry_id" in inquiry &&
                            inquiry.copied_from_inquiry_id &&
                            hasPermission(
                                permissions,
                                "sales_inquiry:detail",
                            ) && (
                                <Link
                                    id="sales-inquiry-copy-source"
                                    href={`/sales/inquiries/${encodeURIComponent(inquiry.copied_from_inquiry_id)}`}
                                    className="text-sm underline underline-offset-4"
                                >
                                    查看原询价
                                </Link>
                            )}
                        {dirty && (
                            <Alert variant="info">
                                <AlertDescription>
                                    当前需求有未保存的修改，请先保存后提交采购。
                                </AlertDescription>
                            </Alert>
                        )}
                        {canEdit && feeOwner}
                    </div>
                    <InquiryDetailTabs
                        feedbackActions={
                            <>
                                {canProcess &&
                                    task &&
                                    hasPermission(
                                        permissions,
                                        "supplier_offering:list",
                                    ) &&
                                    hasPermission(
                                        permissions,
                                        "sellable_sku:list",
                                    ) && (
                                        <SupplierQuoteExportButton
                                            request={{
                                                inquiryId: inquiry.id,
                                                taskId: task.id,
                                                version: inquiry.version,
                                                taskVersion: task.version,
                                            }}
                                            userId={userId}
                                            onRefresh={onSaved}
                                        />
                                    )}
                                {canProcess &&
                                    task &&
                                    "work_items" in inquiry &&
                                    hasPermission(
                                        permissions,
                                        "procurement_inquiry:internal_quote",
                                    ) &&
                                    hasPermission(
                                        permissions,
                                        "supplier_offering:list",
                                    ) &&
                                    hasPermission(
                                        permissions,
                                        "sellable_sku:list",
                                    ) && (
                                        <SupplierQuoteImportButton
                                            inquiry={inquiry}
                                            task={task}
                                            userId={userId}
                                            onSaved={onSaved}
                                        />
                                    )}
                            </>
                        }
                        onFeedbackVisibilityChange={setFeedbackVisible}
                        inquiry={inquiry}
                        procurement={procurement}
                        actionsContainer={historyActionsContainer}
                        feeCount={selectedFeedback?.data.fees.length ?? 0}
                        salesProducts={
                            !procurement && "sales_owner_user_id" in inquiry ? (
                                <SalesInquiryProducts
                                    inquiry={inquiry}
                                    chosenRound={chosenRound}
                                    onRoundChange={setChosenRound}
                                    revision={
                                        revising ? (
                                            <InquiryDemandEditor
                                                inquiry={inquiry}
                                                userId={userId}
                                                permissions={permissions}
                                                storageScope={`${inquiry.id}:reinquiry-demand`}
                                                onSaved={onSaved}
                                                revision={{
                                                    onCancel: () =>
                                                        setRevising(false),
                                                    onSubmitted: () => {
                                                        setRevising(false)
                                                        onSaved()
                                                    },
                                                }}
                                            />
                                        ) : undefined
                                    }
                                    demand={renderDemand(primaryActions)}
                                    footer={renderSalesActions(primaryActions)}
                                />
                            ) : undefined
                        }
                        feedback={
                            <div className="space-y-6">
                                {"work_items" in inquiry && (
                                    <InquiryReinquiryNotice inquiry={inquiry} />
                                )}
                                {canProcess && "work_items" in inquiry && (
                                    <>
                                        <InquiryFeedbackEditor
                                            ref={feedbackEditor}
                                            publishing={publishing}
                                            footerContainer={
                                                feedbackFooterContainer
                                            }
                                            onDirtyChange={setFeedbackDirty}
                                            inquiry={inquiry}
                                            task={task}
                                            userId={userId}
                                            onSaved={onSaved}
                                            footerActions={primaryActions}
                                            onInternalQuote={
                                                hasPermission(
                                                    permissions,
                                                    "procurement_inquiry:internal_quote",
                                                )
                                                    ? setQuoteLineId
                                                    : undefined
                                            }
                                        />
                                        <QuickPreviewSheet
                                            idPrefix="procurement-feedback-internal"
                                            size="preview"
                                            open={quoteLineId !== null}
                                            onOpenChange={(open) => {
                                                if (!open) setQuoteLineId(null)
                                            }}
                                            title={
                                                inquiry.draft.lines.find(
                                                    (line) =>
                                                        line.line_id ===
                                                        quoteLineId,
                                                )?.product_name ?? "供应商报价"
                                            }
                                            description="仅采购可见。采用报价带入供给价，不代表已选定采购供应商。"
                                        >
                                            {quoteLineId && (
                                                <InquiryInternalQuotes
                                                    key={quoteLineId}
                                                    inquiry={inquiry}
                                                    task={task}
                                                    userId={userId}
                                                    permissions={permissions}
                                                    onSaved={onSaved}
                                                    lineId={quoteLineId}
                                                    onAdopt={(quote) =>
                                                        feedbackEditor.current?.adoptQuote(
                                                            quote,
                                                        ) ?? false
                                                    }
                                                />
                                            )}
                                        </QuickPreviewSheet>
                                    </>
                                )}

                                {!canProcess &&
                                    (showReadonlyDemand ? (
                                        renderDemand(primaryActions)
                                    ) : (
                                        <InquiryFeedbackResults
                                            content="products"
                                            inquiry={inquiry}
                                            selectedRound={selectedRound}
                                            onRoundChange={setChosenRound}
                                        />
                                    ))}
                            </div>
                        }
                        publishedFeedback={
                            (canProcess || showReadonlyDemand) &&
                            inquiry.feedbacks.length > 0 ? (
                                <InquiryFeedbackResults
                                    content="products"
                                    inquiry={inquiry}
                                    selectedRound={selectedRound}
                                    onRoundChange={setChosenRound}
                                    idPrefix="procurement-inquiry-published"
                                />
                            ) : undefined
                        }
                        fees={
                            <InquiryFees
                                inquiry={inquiry}
                                selectedRound={selectedRound}
                                onRoundChange={setChosenRound}
                                userId={userId}
                                permissions={permissions}
                                procurement={procurement}
                                handlingEnabled={!procurement || Boolean(task)}
                                onChanged={onSaved}
                            />
                        }
                    />
                    <InquiryImportDialog
                        key={inquiry.id}
                        open={importOpen}
                        onOpenChange={setImportOpen}
                        onPreview={(file) => preview.mutateAsync(file)}
                        busy={preview.isPending || command.isPending}
                        disabled={command.unresolved}
                        onDownloadTemplate={async () => {
                            const products = await safeTemplate.mutateAsync()
                            await downloadInquiryCustomerWorkbook(
                                products,
                                inquiry.inquiry_no,
                            )
                        }}
                        templateBusy={safeTemplate.isPending}
                        onConfirm={async (result, confirm_merge_duplicates) => {
                            await command.run(
                                `${inquiryPath(false, inquiry.id)}/imports/confirm`,
                                {
                                    expected_version: result.expected_version,
                                    preview_id: result.preview_id,
                                    input_fingerprint: result.input_fingerprint,
                                    confirm_merge_duplicates,
                                },
                            )
                            localStorage.removeItem(
                                inquiryStorageKey(
                                    userId,
                                    `${inquiry.id}:demand`,
                                ),
                            )
                            setDirty(false)
                            setEditorEpoch((value) => value + 1)
                            onSaved()
                        }}
                    />
                </div>
            )}
        />
    )
    const content = (
        <>
            {body}
            {canProcess && (
                <div
                    ref={setFeedbackFooterContainer}
                    data-slot="inquiry-feedback-footer"
                    hidden={!feedbackVisible}
                    className="shrink-0"
                />
            )}
        </>
    )
    return embedded ? (
        <div className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden p-6">
            {content}
        </div>
    ) : (
        <PageScaffold className="mx-auto min-h-0 max-w-[1600px] gap-0 overflow-hidden">
            {content}
        </PageScaffold>
    )
}
