"use client"

import { useState } from "react"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    TransferPreviewResumeDialog,
    type InquiryPreviewKind,
    type TransferPreviewData,
} from "@/features/sales-inquiries/components/transfer-preview-resume"
import type {
    InquiryImportConfirmResult,
    InquiryImportPreview,
} from "@/features/sales-inquiries/components/inquiry-import-dialog"
import type { SupplierQuoteImportPreview } from "@/features/sales-inquiries/components/supplier-quote-preview-panel"
import { useTransferPreviewQuery } from "@/features/sales-inquiries/hooks/use-transfer-preview"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DataTransferContext, TransferJob } from "@/lib/data-transfer"
import { useRouter } from "@/lib/navigation"

const stateLabels: Record<DataTransferContext["resume_state"], string> = {
    not_applicable: "无需确认",
    pending: "正在生成预览",
    ready: "预览已生成，可继续核对",
    confirmed: "业务确认已完成",
    invalid: "预览中存在校验问题",
    stale: "业务已变化，预览已失效",
    expired: "预览已过期",
    unavailable: "当前无法继续办理",
}

/** 预览任务区分后台生成结果与业务确认，复杂核对在独立弹窗办理。 */
export function TransferPreviewResult({
    job,
    context,
    portal = false,
    onChanged,
}: {
    job: TransferJob
    context: DataTransferContext
    portal?: boolean
    onChanged?: () => void
}) {
    const [open, setOpen] = useState(false)
    const [confirmedInquiryId, setConfirmedInquiryId] = useState<string>()
    const profile = useAccountProfileQuery()
    const router = useRouter()
    const quote = context.kind === "supplier_quote_preview"
    const canRead =
        !portal &&
        Boolean(job.finished_at && job.result_file_asset_id) &&
        (context.resume_state === "ready" || context.resume_state === "invalid")
    const result = useTransferPreviewQuery<TransferPreviewData>(job.id, canRead)
    const idPrefix = `transfer-preview-${toAutomationIdSegment(job.id)}`
    const inquiryId = confirmedInquiryId ?? context.context.inquiry_id
    const usablePreview = canRead && !result.isError ? result.data : undefined
    const inquiryPreview =
        usablePreview && !quote ? (usablePreview as InquiryImportPreview) : null
    const quotePreview =
        usablePreview && quote
            ? (usablePreview as SupplierQuoteImportPreview)
            : null
    const confirmed = (value?: InquiryImportConfirmResult) => {
        if (value?.inquiry_id) setConfirmedInquiryId(value.inquiry_id)
        setOpen(false)
        onChanged?.()
    }
    const businessPath = inquiryId
        ? `${quote ? "/procurement" : "/sales"}/inquiries/${encodeURIComponent(inquiryId)}${quote && context.context.task_id ? `?task_id=${encodeURIComponent(context.context.task_id)}` : ""}`
        : "/sales/inquiries"
    return (
        <section className="space-y-3">
            <h3 className="font-medium">预览结果与下一步</h3>
            <p className="font-medium">
                {context.resume_state === "confirmed"
                    ? quote
                        ? "供应商报价已登记"
                        : "询价草稿已写入"
                    : stateLabels[context.resume_state]}
            </p>
            {context.context.inquiry_no && (
                <p>关联询价：{context.context.inquiry_no}</p>
            )}
            {context.resume_reason && (
                <p className="text-muted-foreground">{context.resume_reason}</p>
            )}
            {context.resume_state === "ready" && (
                <p className="text-muted-foreground">
                    {quote
                        ? "后台已完成文件解析。核对后确认登记，报价才会写入询价。"
                        : "后台已完成文件解析。核对商品、配送地址和合并结果后，才能完成业务确认。"}
                </p>
            )}
            {quotePreview && (
                <p>
                    解析 {quotePreview.rows.length} 条报价，未报价{" "}
                    {quotePreview.skipped} 行，发现 {quotePreview.issues.length}{" "}
                    项问题。
                </p>
            )}
            {inquiryPreview && (
                <p>
                    解析 {inquiryPreview.data.lines.length} 条商品需求，发现{" "}
                    {inquiryPreview.errors.length} 项问题，
                    {inquiryPreview.merges.length} 组配送行需要合并。
                </p>
            )}
            {context.preview_expires_at && (
                <p className="text-xs text-muted-foreground">
                    业务预览有效期至{" "}
                    {new Date(context.preview_expires_at * 1000).toLocaleString(
                        "zh-CN",
                        { hour12: false },
                    )}
                </p>
            )}
            {result.isError && canRead && (
                <BusinessFailureState
                    error={result.error}
                    action={
                        <Button
                            id={`${idPrefix}-retry`}
                            variant="outline"
                            size="sm"
                            onClick={() => {
                                onChanged?.()
                                void result.refetch()
                            }}
                        >
                            重新读取预览
                        </Button>
                    }
                />
            )}
            <div className="flex flex-wrap gap-2">
                {canRead && (
                    <Button
                        id={`${idPrefix}-resume`}
                        disabled={
                            !usablePreview ||
                            result.isFetching ||
                            !profile.data?.userid
                        }
                        onClick={() => setOpen(true)}
                    >
                        {result.isPending
                            ? "正在读取预览…"
                            : context.resume_state === "invalid"
                              ? "查看校验问题"
                              : quote
                                ? "继续核对报价"
                                : "继续核对导入"}
                    </Button>
                )}
                <Button
                    id={`${idPrefix}-open-business`}
                    variant="outline"
                    onClick={() => router.push(businessPath)}
                >
                    {context.resume_state === "confirmed" || confirmedInquiryId
                        ? quote
                            ? "查看已登记报价"
                            : "查看询价草稿"
                        : inquiryId
                          ? "打开关联询价"
                          : "打开询价列表"}
                </Button>
            </div>
            {open && usablePreview && profile.data?.userid && (
                <TransferPreviewResumeDialog
                    jobId={job.id}
                    kind={context.kind as InquiryPreviewKind}
                    context={context}
                    preview={usablePreview}
                    userId={profile.data.userid}
                    open={open}
                    onOpenChange={setOpen}
                    onConfirmed={confirmed}
                />
            )}
        </section>
    )
}
