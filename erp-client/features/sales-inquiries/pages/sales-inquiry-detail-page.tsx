"use client"

import * as React from "react"
import Link from "next/link"
import { useSearchParams } from "next/navigation"
import { useMutation } from "@tanstack/react-query"
import { UploadIcon } from "lucide-react"
import {
    BusinessFailureState,
    DetailPageHeader,
    PageScaffold,
} from "@/components/business"
import { Button, buttonVariants } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { apiPost, apiPostForm } from "@/lib/api"
import { hasPermission } from "@/lib/permissions"
import { inquiryPath } from "../api"
import { useInquiry } from "../hooks/queries"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { inquiryDate } from "../lib/format"
import { inquiryStorageKey } from "../lib/local-draft"
import {
    downloadInquiryCustomerWorkbook,
    type InquiryCustomerWorkbookProduct,
} from "../lib/customer-workbook"
import { InquiryDetailTabs } from "../components/inquiry-detail-tabs"
import { InquiryFeedbackResults } from "../components/inquiry-feedback-results"
import { InquiryDemandEditor } from "../components/inquiry-demand-editor"
import { InquiryFeedbackEditor } from "../components/inquiry-feedback-editor"
import { InquiryActions } from "../components/inquiry-actions"
import { InquiryInternalQuotes } from "../components/inquiry-internal-quotes"
import { InquiryFeesPanel } from "../components/inquiry-fees-panel"
import { InquiryRequirementTable } from "../components/inquiry-requirement-table"
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
}: {
    inquiryId?: string
    procurement?: boolean
    workItemId?: string
    embedded?: boolean
}) {
    const search = useSearchParams()
    const taskId =
        workItemId ??
        search.get("task_id") ??
        search.get("workItemId") ??
        undefined
    const copyFromId = !inquiryId ? (search.get("copyFrom") ?? "") : ""
    const profile = useAccountProfileQuery()
    const query = useInquiry(inquiryId || copyFromId, procurement, taskId)
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
    if (query.isError)
        return (
            <div className="p-6">
                <BusinessFailureState
                    id="sales-inquiry-detail-retry"
                    title="询价资料加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
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
            onSaved={() => void query.refetch()}
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
    const [feedbackDirty, setFeedbackDirty] = React.useState(false)
    const [quoteLineId, setQuoteLineId] = React.useState<string | null>(null)
    const [editorEpoch, setEditorEpoch] = React.useState(0)
    const [demandDetailsContainer, setDemandDetailsContainer] =
        React.useState<HTMLDivElement | null>(null)
    const [demandActionsContainer, setDemandActionsContainer] =
        React.useState<HTMLDivElement | null>(null)
    const [chosenRound, setChosenRound] = React.useState(inquiry.feedback_round)
    const selectedRound = inquiry.feedbacks.some(
        (item) => item.round === chosenRound,
    )
        ? chosenRound
        : inquiry.feedback_round
    const demandActions = ["draft", "awaiting_sales_supplement"].includes(
        inquiry.status,
    )
    const command = useInquiryCommand(userId, inquiry.id)
    const task =
        "work_items" in inquiry
            ? (inquiry.work_items.find((item) => item.id === taskId) ??
              inquiry.work_items[0])
            : undefined
    const canEdit =
        !procurement &&
        ["draft", "awaiting_sales_supplement"].includes(inquiry.status) &&
        hasPermission(permissions, "sales_inquiry:update") &&
        hasPermission(permissions, "sellable_sku:list")
    const canImport =
        canEdit && hasPermission(permissions, "sales_inquiry:import")
    const canProcess =
        procurement &&
        task &&
        ["awaiting_procurement", "procurement_processing"].includes(
            inquiry.status,
        ) &&
        hasPermission(permissions, "procurement_inquiry:save_feedback")
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
    const body = (
        <InquiryActions
            inquiry={inquiry}
            procurement={procurement}
            task={task}
            userId={userId}
            permissions={permissions}
            onSaved={onSaved}
            disabled={dirty || feedbackDirty}
            feedbackRound={selectedRound}
            renderActions={(
                primaryActions,
                secondaryActions,
                assignmentActions,
            ) => (
                <div className="space-y-5">
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
                            hasPermission(permissions, "sellable_sku:list") ? (
                                <Link
                                    id="sales-inquiry-copy-new"
                                    href={`/sales/inquiries/new?copyFrom=${encodeURIComponent(inquiry.id)}`}
                                    className={buttonVariants({ size: "sm" })}
                                >
                                    复制为新询价
                                </Link>
                            ) : undefined
                        }
                        meta={
                            canEdit ? undefined : (
                                <span>
                                    询价截止：{inquiryDate(inquiry.deadline_at)}
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
                                            需求说明
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
                        hasPermission(permissions, "sales_inquiry:detail") && (
                            <Link
                                id="sales-inquiry-copy-source"
                                href={`/sales/inquiries/${encodeURIComponent(inquiry.copied_from_inquiry_id)}`}
                                className="text-sm underline underline-offset-4"
                            >
                                查看原询价
                            </Link>
                        )}
                    {feedbackDirty && (
                        <Alert variant="info">
                            <AlertDescription>
                                采购反馈有未保存的修改，请先保存处理进度，再发布或退回补充。
                            </AlertDescription>
                        </Alert>
                    )}
                    {dirty && (
                        <Alert variant="info">
                            <AlertDescription>
                                当前需求有未保存的修改，请先保存后提交采购。
                            </AlertDescription>
                        </Alert>
                    )}
                    {canEdit && feeOwner}
                    <InquiryDetailTabs
                        inquiry={inquiry}
                        procurement={procurement}
                        actions={assignmentActions}
                        demand={
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
                                        detailsContainer={
                                            demandDetailsContainer
                                        }
                                        headerActionsContainer={
                                            demandActionsContainer
                                        }
                                        footerActions={
                                            demandActions
                                                ? primaryActions
                                                : undefined
                                        }
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
                                    React.Children.count(primaryActions) >
                                        0 && (
                                        <div className="sticky bottom-0 flex flex-wrap justify-end gap-3 border-t bg-card py-4">
                                            {primaryActions}
                                        </div>
                                    )}
                            </div>
                        }
                        feedback={
                            <div className="space-y-6">
                                {canProcess && "work_items" in inquiry && (
                                    <>
                                        <InquiryFeedbackEditor
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
                                        <Dialog
                                            open={quoteLineId !== null}
                                            onOpenChange={(open) => {
                                                if (!open) setQuoteLineId(null)
                                            }}
                                        >
                                            <DialogContent
                                                id="procurement-feedback-internal-dialog"
                                                closeButtonId="procurement-feedback-internal-close"
                                                className="max-h-[85vh] overflow-y-auto sm:max-w-4xl"
                                            >
                                                <DialogHeader>
                                                    <DialogTitle>
                                                        {
                                                            inquiry.draft.lines.find(
                                                                (line) =>
                                                                    line.line_id ===
                                                                    quoteLineId,
                                                            )?.product_name
                                                        }{" "}
                                                        · 内部采购信息
                                                    </DialogTitle>
                                                    <DialogDescription>
                                                        仅采购可见；供应商报价独立保存，不随销售反馈公开。
                                                    </DialogDescription>
                                                </DialogHeader>
                                                {quoteLineId && (
                                                    <InquiryInternalQuotes
                                                        key={quoteLineId}
                                                        inquiry={inquiry}
                                                        task={task}
                                                        userId={userId}
                                                        permissions={
                                                            permissions
                                                        }
                                                        onSaved={onSaved}
                                                        lineId={quoteLineId}
                                                    />
                                                )}
                                            </DialogContent>
                                        </Dialog>
                                    </>
                                )}

                                <InquiryFeedbackResults
                                    inquiry={inquiry}
                                    selectedRound={selectedRound}
                                    onRoundChange={setChosenRound}
                                />
                                {!canProcess &&
                                    !demandActions &&
                                    (React.Children.count(primaryActions) > 0 ||
                                        canOpenSales) && (
                                        <div className="sticky bottom-0 flex flex-wrap items-center justify-end gap-3 border-t bg-card py-4">
                                            {selectedRound !==
                                                inquiry.feedback_round && (
                                                <p className="mr-auto text-sm text-muted-foreground">
                                                    正在查看历史轮次，请切回最新一轮后接受或重询。
                                                </p>
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
                                                    按第{" "}
                                                    {accepted.feedback_round}{" "}
                                                    轮已接受结果开单
                                                </Link>
                                            )}
                                        </div>
                                    )}
                            </div>
                        }
                        fees={
                            <InquiryFeesPanel
                                inquiry={inquiry}
                                userId={userId}
                                permissions={permissions}
                                procurement={procurement}
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
    return embedded ? (
        <div className="min-w-0 p-6">{body}</div>
    ) : (
        <PageScaffold className="mx-auto max-w-[1600px] gap-6">
            {body}
        </PageScaffold>
    )
}
