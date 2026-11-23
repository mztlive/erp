"use client"

import * as React from "react"
import Link from "next/link"
import { useMutation } from "@tanstack/react-query"
import { UploadIcon } from "lucide-react"
import {
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspacePage,
    DetailWorkspaceSaveDock,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button, buttonVariants } from "@/components/ui/button"
import { apiPost } from "@/lib/api"
import { transferFileInput, transferJson } from "@/lib/data-transfer"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { inquiryPath } from "../api"
import {
    useInquiryCommand,
    useInquiryCommandResolution,
} from "../hooks/use-inquiry-command"
import {
    downloadInquiryCustomerWorkbook,
    type InquiryCustomerWorkbookProduct,
    withSavedDemand,
} from "../lib/customer-workbook"
import { summarizeDemand } from "../lib/demand-summary"
import { summarizeFeedback, sumInquiryFeesGross } from "../lib/feedback-summary"
import { salesInquiryFeedback } from "../lib/feedback-view"
import { inquiryDateInput, inquiryDeadlineMeta } from "../lib/format"
import { inquiryStorageKey } from "../lib/local-draft"
import { resolveInquiryBackLink } from "../lib/navigation"
import {
    INQUIRY_STATUS_LABEL,
    INQUIRY_STATUS_TONE,
    type SalesInquiry,
} from "../types"
import { InquiryActions } from "./inquiry-actions"
import {
    InquiryDemandEditor,
    type InquiryDemandEditorHandle,
    type InquiryDemandEditorStatus,
} from "./inquiry-demand-editor"
import { InquiryDemandOverview } from "./inquiry-demand-header"
import { InquiryDetailTabs, InquiryHistoryButton } from "./inquiry-detail-tabs"
import { InquiryFees } from "./inquiry-fees"
import {
    InquiryCustomerTemplateButton,
    InquiryImportDialog,
    type InquiryImportPreview,
} from "./inquiry-import-dialog"
import { TRANSFER_LABEL, VALIDITY_LABEL } from "./inquiry-progress"
import { InquiryRequirementTable } from "./inquiry-requirement-table"
import { SalesInquiryProducts } from "./sales-inquiry-products"

export function InquirySalesView({
    inquiry,
    userId,
    permissions,
    returnTo,
    onSaved,
}: {
    inquiry: SalesInquiry
    userId: string
    permissions: readonly string[]
    returnTo?: string | null
    onSaved: () => void
}) {
    const [importOpen, setImportOpen] = React.useState(false)
    const [dirty, setDirty] = React.useState(false)
    const [revising, setRevising] = React.useState(false)
    const [editorEpoch, setEditorEpoch] = React.useState(0)
    const demandEditorRef = React.useRef<InquiryDemandEditorHandle>(null)
    const [demandStatus, setDemandStatus] =
        React.useState<InquiryDemandEditorStatus>(() => {
            const initialSummary = summarizeDemand({
                customer_id: inquiry.customer_id,
                customer_name: inquiry.customer_name,
                sales_owner_user_id: inquiry.sales_owner_user_id,
                business_org_unit_id: inquiry.business_org_unit_id,
                deadline_at: inquiry.deadline_at,
                draft: inquiry.draft,
            })
            return {
                dirty: false,
                saving: false,
                disabled: false,
                lineCount: initialSummary.lineCount,
                pendingDeliveryCount: initialSummary.pendingDeliveryCount,
                canSave: false,
                customerId: inquiry.customer_id,
                deadlineAt: inquiry.deadline_at,
                pricedLineCount: initialSummary.pricedLineCount,
                referenceTotal: initialSummary.referenceTotal,
            }
        })

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
    const productFeedback = salesInquiryFeedback(inquiry, chosenRound)
    const actionRound = productFeedback?.round ?? inquiry.feedback_round
    const command = useInquiryCommand(userId, inquiry.id)

    const canEdit =
        ["draft", "awaiting_sales_supplement"].includes(inquiry.status) &&
        hasPermission(permissions, "sales_inquiry:update") &&
        hasPermission(permissions, "sellable_sku:list")
    const canRevise =
        ["awaiting_sales", "confirmed"].includes(inquiry.status) &&
        hasPermission(permissions, "sales_inquiry:reinquire") &&
        hasPermission(permissions, "sellable_sku:list")
    const canImport =
        canEdit && hasPermission(permissions, "sales_inquiry:import")
    const accepted = inquiry.acceptances.at(-1)
    const canOpenSales =
        inquiry.sales_progress.can_create_sales &&
        accepted != null &&
        hasPermission(permissions, "sales_inquiry:convert_sales") &&
        hasPermission(permissions, "sales_order:create")
    const canCopyClosed =
        inquiry.status === "closed" &&
        hasPermission(permissions, "sales_inquiry:create") &&
        hasPermission(permissions, "sellable_sku:list")

    useWorkspaceDirtyState({
        dirty: (canEdit && dirty) || revising,
        busy: demandStatus.saving || command.isPending,
    })

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

    const safeTemplate = useMutation({
        mutationFn: async () => {
            const lines = inquiry.draft.lines
            const products = await apiPost<InquiryCustomerWorkbookProduct[]>(
                `${inquiryPath()}/template-products`,
                {
                    sku_ids: [...new Set(lines.map((line) => line.sku_id))],
                },
            )
            return withSavedDemand(products, lines)
        },
    })
    const preview = useMutation({
        mutationFn: async (file: File) =>
            transferJson<InquiryImportPreview>("inquiry_preview", {
                ...(await transferFileInput(file)),
                inquiry_id: inquiry.id,
                expected_version: inquiry.version,
            }),
        meta: { suppressErrorToast: true },
    })

    const feeOwner = inquiry.shared_fee_owner_user_id ? (
        <p className="text-xs text-muted-foreground">
            整单费用责任：
            {inquiry.shared_fee_owner_name ||
                (inquiry.shared_fee_owner_user_id === userId
                    ? "由我维护"
                    : "已指定责任采购")}
        </p>
    ) : null

    const copySourceHref =
        inquiry.copied_from_inquiry_id &&
        hasPermission(permissions, "sales_inquiry:detail")
            ? `/sales/inquiries/${encodeURIComponent(inquiry.copied_from_inquiry_id)}`
            : undefined

    const productActions = (
        <>
            {inquiry.draft.lines.length > 0 &&
                hasPermission(permissions, "sales_inquiry:create") &&
                hasPermission(permissions, "sellable_sku:list") && (
                    <InquiryCustomerTemplateButton
                        id="sales-inquiry-download-template"
                        size="sm"
                        variant="ghost"
                        buttonClassName={detailWorkspaceStyles.subtleButton}
                        productCount={inquiry.draft.lines.length}
                        onLoadProducts={() => safeTemplate.mutateAsync()}
                        inquiry_no={inquiry.inquiry_no}
                        disabled={inquiry.status === "closed"}
                    />
                )}
            {canImport && (
                <Button
                    id="sales-inquiry-detail-import"
                    variant="ghost"
                    size="sm"
                    className={detailWorkspaceStyles.subtleButton}
                    type="button"
                    onClick={() => setImportOpen(true)}
                >
                    <UploadIcon aria-hidden="true" data-icon="inline-start" />
                    导入客户选品
                </Button>
            )}
        </>
    )

    const back = resolveInquiryBackLink({
        id: "sales-inquiry-detail-back",
        procurement: false,
        returnTo,
    })

    const saveDockOpen = canEdit && dirty && demandStatus.canSave
    const isActiveInquiry = !["confirmed", "closed"].includes(inquiry.status)
    const deadline = inquiryDeadlineMeta(inquiry.deadline_at, isActiveInquiry)
    const editingDeadlineText =
        inquiryDateInput(demandStatus.deadlineAt) || "未设置"
    const feeOwnerLabel = inquiry.shared_fee_owner_user_id
        ? inquiry.shared_fee_owner_name ||
          (inquiry.shared_fee_owner_user_id === userId
              ? "由我维护"
              : "已指定责任采购")
        : null
    const headerMetaItems = [
        inquiry.requirement_version > 0 && (
            <span key="req" className="num">
                需求第 {inquiry.requirement_version} 版
            </span>
        ),
        inquiry.feedback_round > 0 && (
            <span key="round" className="num">
                反馈第 {inquiry.feedback_round} 轮
            </span>
        ),
        inquiry.deadline_at > 0 && (
            <span key="deadline">
                截止：<span className="num">{deadline.label}</span>
            </span>
        ),
        feeOwnerLabel && (
            <span key="fee-owner">
                整单费用：
                <span className="text-foreground/85">{feeOwnerLabel}</span>
            </span>
        ),
    ].filter(Boolean)
    const feeCount = selectedFeedback?.data.fees.length ?? 0
    const feeTotal = sumInquiryFeesGross(selectedFeedback?.data.fees ?? [])

    const productRequirementLines =
        (productFeedback
            ? inquiry.requirements.find(
                  (item) =>
                      item.version === productFeedback.requirement_version,
              )?.data.lines
            : undefined) ?? inquiry.draft.lines
    const feedbackSummary = React.useMemo(
        () =>
            productFeedback
                ? summarizeFeedback(
                      productFeedback.data.lines,
                      productRequirementLines,
                      productFeedback.data.fees,
                  )
                : null,
        [productFeedback, productRequirementLines],
    )

    const availableCount = feedbackSummary?.availableCount ?? 0
    const partialCount = feedbackSummary?.partialCount ?? 0
    const unavailableCount = feedbackSummary?.unavailableCount ?? 0

    const summaryBar = revising ? undefined : canEdit ? (
        <DetailWorkspaceMetrics columns={4} variant="bar" ariaLabel="询价概览">
            <DetailWorkspaceMetricItem
                label="商品需求"
                emphasis="primary"
                value={
                    <span>
                        <span className="num">{demandStatus.lineCount}</span>{" "}
                        <span className="text-body-compact font-normal text-muted-foreground">
                            项
                        </span>
                    </span>
                }
                detail={
                    demandStatus.lineCount === 0
                        ? "尚未添加商品"
                        : demandStatus.pendingDeliveryCount === 0
                          ? "配送资料已完善"
                          : `${demandStatus.pendingDeliveryCount} 项待完善配送`
                }
            />
            <DetailWorkspaceMetricItem
                label="销售参考合计"
                value={
                    demandStatus.referenceTotal !== null ? (
                        <MoneyValue
                            value={demandStatus.referenceTotal}
                            display="exact"
                            showHint={false}
                        />
                    ) : (
                        <span className="text-muted-foreground">—</span>
                    )
                }
                detail={
                    demandStatus.pricedLineCount > 0
                        ? `已填 ${demandStatus.pricedLineCount} / ${demandStatus.lineCount} 项`
                        : "销售价为选填项"
                }
            />
            <DetailWorkspaceMetricItem
                label="询价截止"
                value={editingDeadlineText}
                detail={
                    inquiry.requirement_version > 0
                        ? `需求第 ${inquiry.requirement_version} 版`
                        : "草稿尚未提交"
                }
            />
            <DetailWorkspaceMetricItem
                label="反馈轮次"
                value={
                    inquiry.feedback_round > 0
                        ? `第 ${inquiry.feedback_round} 轮`
                        : "待提交采购"
                }
                detail={
                    inquiry.status === "awaiting_sales_supplement"
                        ? "采购退回待补充"
                        : "完善需求后提交采购"
                }
            />
        </DetailWorkspaceMetrics>
    ) : (
        <DetailWorkspaceMetrics columns={4} variant="bar" ariaLabel="询价概览">
            <DetailWorkspaceMetricItem
                label="商品与反馈"
                emphasis="primary"
                value={
                    <span>
                        <span className="num">
                            {inquiry.draft.lines.length}
                        </span>{" "}
                        <span className="text-body-compact font-normal text-muted-foreground">
                            项
                        </span>
                    </span>
                }
                detail={
                    productFeedback
                        ? `可供 ${availableCount} · 部分 ${partialCount} · 不供应 ${unavailableCount}`
                        : "等待采购反馈"
                }
            />
            <DetailWorkspaceMetricItem
                label="含税供给合计"
                value={
                    feedbackSummary?.goodsTotal != null ? (
                        <MoneyValue
                            value={feedbackSummary.goodsTotal}
                            display="exact"
                            showHint={false}
                        />
                    ) : (
                        <span className="text-muted-foreground">—</span>
                    )
                }
                detail={
                    feeTotal !== null
                        ? `另含额外费用 ¥${feeTotal}（${feeCount} 项）`
                        : feeCount > 0
                          ? `共 ${feeCount} 项额外费用`
                          : "暂无额外费用"
                }
            />
            <DetailWorkspaceMetricItem
                label="转单进度"
                value={
                    <span>
                        <span className="num">
                            {inquiry.sales_progress.completed_lines}
                        </span>
                        <span className="text-muted-foreground"> / </span>
                        <span className="num">
                            {inquiry.sales_progress.total_lines}
                        </span>
                    </span>
                }
                detail={`${TRANSFER_LABEL[inquiry.sales_progress.transfer_status]} · ${VALIDITY_LABEL[inquiry.sales_progress.quote_validity]}`}
            />
            <DetailWorkspaceMetricItem
                label="询价截止"
                value={deadline.label}
                detail={
                    <span className="flex flex-wrap items-center gap-x-1.5">
                        {deadline.hint ? (
                            <span
                                className={cn(
                                    deadline.tone === "destructive" &&
                                        "font-medium text-destructive",
                                    deadline.tone === "warning" &&
                                        "font-medium text-warning-soft-foreground",
                                )}
                            >
                                {deadline.hint}
                            </span>
                        ) : null}
                        {deadline.hint ? (
                            <span
                                aria-hidden="true"
                                className="text-muted-foreground/40"
                            >
                                ·
                            </span>
                        ) : null}
                        <span className="num">
                            {inquiry.feedback_round > 0
                                ? `需求第 ${inquiry.requirement_version} 版 · 第 ${inquiry.feedback_round} 轮`
                                : `需求第 ${inquiry.requirement_version} 版 · 待首次反馈`}
                        </span>
                    </span>
                }
            />
        </DetailWorkspaceMetrics>
    )

    return (
        <DetailWorkspacePage>
            <InquiryActions
                workspace
                inquiry={inquiry}
                procurement={false}
                userId={userId}
                permissions={permissions}
                onSaved={onSaved}
                disabled={dirty || revising}
                disabledReason={
                    revising ? "请先提交或取消需求修改" : "请先保存当前需求修改"
                }
                feedbackRound={actionRound}
                renderActions={({
                    primaryAction: workflowPrimary,
                    secondaryActions: workflowSecondary,
                    alerts: actionAlerts,
                }) => {
                    const createSaleHref =
                        canOpenSales && accepted
                            ? `/sales/orders?mode=create&inquiryId=${encodeURIComponent(inquiry.id)}&round=${accepted.feedback_round}`
                            : null
                    const primaryAction = canCopyClosed ? (
                        <Link
                            id="sales-inquiry-copy-new"
                            href={`/sales/inquiries/new?copyFrom=${encodeURIComponent(inquiry.id)}`}
                            className={buttonVariants({
                                size: "sm",
                                className: "px-3.5",
                            })}
                        >
                            复制为新询价
                        </Link>
                    ) : (
                        (workflowPrimary ??
                        (createSaleHref && accepted ? (
                            <Link
                                id="sales-inquiry-create-sale"
                                href={createSaleHref}
                                className={buttonVariants({
                                    size: "sm",
                                    className: "px-3.5",
                                })}
                            >
                                按第 {accepted.feedback_round} 轮已接受结果开单
                            </Link>
                        ) : undefined))
                    )

                    const secondaryCreateSale =
                        workflowPrimary && createSaleHref && accepted ? (
                            <Link
                                id="sales-inquiry-create-sale"
                                href={createSaleHref}
                                className={buttonVariants({
                                    variant: "outline",
                                    size: "sm",
                                })}
                            >
                                按第 {accepted.feedback_round} 轮已接受结果开单
                            </Link>
                        ) : null

                    const modifyDemandButton =
                        canRevise && !revising ? (
                            <Button
                                id="sales-inquiry-modify-demand"
                                type="button"
                                variant="ghost"
                                className={detailWorkspaceStyles.quietButton}
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
                        ) : null

                    const alerts = (
                        <>
                            {actionAlerts}
                            {canOpenSales &&
                                accepted &&
                                inquiry.status !== "confirmed" && (
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
                            {!canEdit &&
                                actionRound !== inquiry.feedback_round && (
                                    <Alert variant="info">
                                        <AlertDescription>
                                            正在查看历史轮次，请切回最新一轮后接受或重新询价。
                                        </AlertDescription>
                                    </Alert>
                                )}
                        </>
                    )

                    return (
                        <>
                            <InquiryDetailTabs
                                workspace
                                inquiry={inquiry}
                                procurement={false}
                                feeCount={feeCount}
                                renderLayout={(navigation, panels) => (
                                    <>
                                        <DetailWorkspaceChrome
                                            title={inquiry.customer_name}
                                            documentNumber={inquiry.inquiry_no}
                                            numberLabel="询价单号"
                                            primaryStatus={{
                                                label: INQUIRY_STATUS_LABEL[
                                                    inquiry.status
                                                ],
                                                tone: INQUIRY_STATUS_TONE[
                                                    inquiry.status
                                                ],
                                            }}
                                            titleExtra={
                                                dirty || revising ? (
                                                    <DetailWorkspaceDirtyBadge
                                                        label={
                                                            revising
                                                                ? "正在修改需求"
                                                                : "有未保存修改"
                                                        }
                                                    />
                                                ) : undefined
                                            }
                                            meta={
                                                headerMetaItems.length > 0 ? (
                                                    <DetailWorkspaceMeta
                                                        items={headerMetaItems}
                                                    />
                                                ) : undefined
                                            }
                                            back={back}
                                            secondaryActions={
                                                <>
                                                    <InquiryHistoryButton
                                                        inquiry={inquiry}
                                                        workspace
                                                    />
                                                    {modifyDemandButton}
                                                    {secondaryCreateSale}
                                                    {workflowSecondary}
                                                </>
                                            }
                                            primaryAction={primaryAction}
                                            tabs={navigation}
                                        />
                                        <DetailWorkspaceBody
                                            summary={summaryBar}
                                            alerts={alerts}
                                            saveDockOpen={saveDockOpen}
                                        >
                                            {!canEdit && !revising && (
                                                <InquiryDemandOverview
                                                    inquiry={inquiry}
                                                    userId={userId}
                                                    isCurrentNote={
                                                        !productFeedback ||
                                                        productFeedback.requirement_version ===
                                                            inquiry.requirement_version
                                                    }
                                                    copySourceHref={
                                                        copySourceHref
                                                    }
                                                />
                                            )}
                                            {panels}
                                        </DetailWorkspaceBody>
                                    </>
                                )}
                                salesProducts={
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
                                        demand={
                                            canEdit ? (
                                                <InquiryDemandEditor
                                                    ref={demandEditorRef}
                                                    key={`editor-${editorEpoch}`}
                                                    inquiry={inquiry}
                                                    userId={userId}
                                                    permissions={permissions}
                                                    onSaved={onSaved}
                                                    onDirtyChange={setDirty}
                                                    onStatusChange={
                                                        setDemandStatus
                                                    }
                                                    productActions={
                                                        productActions
                                                    }
                                                    feeOwner={feeOwner}
                                                />
                                            ) : (
                                                <InquiryRequirementTable
                                                    idPrefix="sales-inquiry-requirement"
                                                    requirement={inquiry.draft}
                                                />
                                            )
                                        }
                                    />
                                }
                                fees={
                                    <InquiryFees
                                        inquiry={inquiry}
                                        selectedRound={selectedRound}
                                        onRoundChange={setChosenRound}
                                        userId={userId}
                                        permissions={permissions}
                                        procurement={false}
                                        handlingEnabled
                                        onChanged={onSaved}
                                    />
                                }
                            />
                            {canEdit && (
                                <DetailWorkspaceSaveDock
                                    open={saveDockOpen}
                                    changeCount={0}
                                    description={
                                        demandStatus.pendingDeliveryCount > 0
                                            ? `${demandStatus.pendingDeliveryCount} 项待完善配送 · 保存后可提交采购`
                                            : "保存后可提交采购"
                                    }
                                    onSave={() =>
                                        demandEditorRef.current?.save()
                                    }
                                    saving={demandStatus.saving}
                                    actions={
                                        <Button
                                            id="sales-inquiry-save-demand"
                                            type="button"
                                            variant="default"
                                            size="sm"
                                            disabled={demandStatus.disabled}
                                            onClick={() =>
                                                demandEditorRef.current?.save()
                                            }
                                        >
                                            {demandStatus.saving
                                                ? "正在保存…"
                                                : "保存需求"}
                                        </Button>
                                    }
                                />
                            )}
                            <InquiryImportDialog
                                key={inquiry.id}
                                open={importOpen}
                                onOpenChange={setImportOpen}
                                onPreview={(file) => preview.mutateAsync(file)}
                                busy={preview.isPending || command.isPending}
                                disabled={command.unresolved}
                                onDownloadTemplate={async () => {
                                    const products =
                                        await safeTemplate.mutateAsync()
                                    await downloadInquiryCustomerWorkbook(
                                        products,
                                        inquiry.inquiry_no,
                                    )
                                }}
                                templateBusy={safeTemplate.isPending}
                                onConfirm={async (
                                    result,
                                    confirm_merge_duplicates,
                                ) => {
                                    await command.run(
                                        `${inquiryPath(false, inquiry.id)}/imports/confirm`,
                                        {
                                            expected_version:
                                                result.expected_version,
                                            preview_id: result.preview_id,
                                            input_fingerprint:
                                                result.input_fingerprint,
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
                        </>
                    )
                }}
            />
        </DetailWorkspacePage>
    )
}
