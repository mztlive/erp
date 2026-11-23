"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"
import { fetchSettlementReviewerOptions } from "../api/reviewer-options"

import {
    FormalActionConfirmDialog,
    MoneyValue,
    OptionCombobox,
} from "@/components/business"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { LoadingButton } from "@/components/ui/loading-button"
import { RequiredMark } from "@/components/ui/required-mark"
import { Textarea } from "@/components/ui/textarea"
import {
    RESOLUTION_LABEL,
    type DifferenceResolution,
    type SettlementDetailView,
    type SettlementDifferenceView,
} from "@/features/supplier-settlements/types"
import { describeMoney } from "@/lib/money-display"

const REJECT_COMMENT_MAX_CHARS = 500
const REJECT_QUICK_PHRASES = [
    "缺少供应商盖章对账单",
    "部分订单金额与采购单不一致",
    "请补充差异核对凭证",
] as const

function MoneyWithReadout({
    label,
    value,
}: {
    label: string
    value: string | null | undefined
}) {
    const readout = describeMoney(value)?.readout
    return (
        <span className="inline-flex flex-wrap items-baseline gap-1.5">
            <span>{label}：</span>
            <MoneyValue
                value={value}
                display="exact"
                className="font-semibold"
            />
            {readout ? (
                <span className="text-xs text-muted-foreground">
                    （{readout}）
                </span>
            ) : null}
        </span>
    )
}

function SettlementResolveDialog({
    open,
    onOpenChange,
    activeDiff,
    resolution,
    onResolutionChange,
    reasonCode,
    onReasonCodeChange,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    activeDiff?: SettlementDifferenceView | null
    resolution: DifferenceResolution
    onResolutionChange: (resolution: DifferenceResolution) => void
    reasonCode: string
    onReasonCodeChange: (reasonCode: string) => void
    pending: boolean
    onSubmit: () => void
}) {
    const diffReadout = describeMoney(activeDiff?.amountGross)?.readout
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && pending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                size="md"
                scroll="body"
                closeButtonId="supplier-settlements-resolve-close"
                showCloseButton={!pending}
            >
                <DialogHeader>
                    <DialogTitle>登记差异处理结论</DialogTitle>
                    <DialogDescription>
                        登记后将更新待确认成本差额，结论不可撤回；原始证据和历史成本保留。
                    </DialogDescription>
                </DialogHeader>
                <DialogBody className="space-y-4">
                    {activeDiff ? (
                        <dl className="grid gap-2.5 rounded-lg border border-border/70 bg-muted/35 p-3 text-xs sm:grid-cols-3">
                            <div className="sm:col-span-3">
                                <dt className="text-muted-foreground">
                                    当前目标差异
                                </dt>
                                <dd className="mt-0.5 flex flex-wrap items-baseline gap-2 font-medium text-foreground">
                                    <span>{activeDiff.typeLabel}</span>
                                    {activeDiff.amountGross ? (
                                        <>
                                            <MoneyValue
                                                value={activeDiff.amountGross}
                                                display="exact"
                                                className="text-warning-soft-foreground"
                                            />
                                            {diffReadout ? (
                                                <span className="font-normal text-muted-foreground">
                                                    （{diffReadout}）
                                                </span>
                                            ) : null}
                                        </>
                                    ) : null}
                                </dd>
                            </div>
                            <div>
                                <dt className="text-muted-foreground">
                                    ERP 侧金额
                                </dt>
                                <dd className="mt-0.5">
                                    {activeDiff.erpSideAmount ? (
                                        <MoneyValue
                                            value={activeDiff.erpSideAmount}
                                            display="exact"
                                        />
                                    ) : (
                                        <span>
                                            {activeDiff.erpSideLabel || "—"}
                                        </span>
                                    )}
                                </dd>
                            </div>
                            <div>
                                <dt className="text-muted-foreground">
                                    供应商侧金额
                                </dt>
                                <dd className="mt-0.5">
                                    {activeDiff.supplierSideAmount ? (
                                        <MoneyValue
                                            value={
                                                activeDiff.supplierSideAmount
                                            }
                                            display="exact"
                                        />
                                    ) : (
                                        <span>
                                            {activeDiff.supplierSideLabel ||
                                                "—"}
                                        </span>
                                    )}
                                </dd>
                            </div>
                            <div>
                                <dt className="text-muted-foreground">
                                    差异方向
                                </dt>
                                <dd className="mt-0.5 text-foreground">
                                    {activeDiff.amountDirectionLabel || "—"}
                                </dd>
                            </div>
                        </dl>
                    ) : null}
                    <div className="space-y-3">
                        <div className="space-y-1.5">
                            <Label htmlFor="supplier-settlements-resolve-resolution">
                                处理结论
                                <RequiredMark />
                            </Label>
                            <OptionCombobox
                                id="supplier-settlements-resolve-resolution"
                                value={resolution}
                                onValueChange={(v) => {
                                    if (v)
                                        onResolutionChange(
                                            v as DifferenceResolution,
                                        )
                                }}
                                options={(
                                    Object.keys(
                                        RESOLUTION_LABEL,
                                    ) as DifferenceResolution[]
                                ).map((k) => ({
                                    value: k,
                                    label: RESOLUTION_LABEL[k],
                                }))}
                                allowClear={false}
                            />
                        </div>
                        <div className="space-y-1.5">
                            <Label htmlFor="supplier-settlements-resolve-reason">
                                处理原因
                                <RequiredMark />
                            </Label>
                            <OptionCombobox
                                id="supplier-settlements-resolve-reason"
                                value={reasonCode}
                                onValueChange={(v) => {
                                    if (v) onReasonCodeChange(v)
                                }}
                                options={[
                                    {
                                        value: "BILL_ALIGNED",
                                        label: "账单已对齐",
                                    },
                                    {
                                        value: "ACCEPT_BILL",
                                        label: "接受供应商账单",
                                    },
                                    {
                                        value: "NO_BUSINESS_IMPACT",
                                        label: "无需业务调整",
                                    },
                                    {
                                        value: "COMPENSATED_ELSEWHERE",
                                        label: "已另行补偿",
                                    },
                                ]}
                                allowClear={false}
                            />
                        </div>
                    </div>
                </DialogBody>
                <DialogFooter>
                    <Button
                        id="supplier-settlements-resolve-cancel"
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <LoadingButton
                        id="supplier-settlements-resolve-confirm"
                        loading={pending}
                        type="button"
                        disabled={pending}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? "提交中…" : "提交结论"}
                    </LoadingButton>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function SettlementEvidenceDialog({
    open,
    onOpenChange,
    referenceId,
    onReferenceIdChange,
    comment,
    onCommentChange,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    referenceId: string
    onReferenceIdChange: (referenceId: string) => void
    comment: string
    onCommentChange: (comment: string) => void
    pending: boolean
    onSubmit: () => void
}) {
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && pending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                size="md"
                scroll="body"
                closeButtonId="supplier-settlements-evidence-close"
                showCloseButton={!pending}
            >
                <DialogHeader>
                    <DialogTitle>追加采购协同证据</DialogTitle>
                    <DialogDescription>
                        登记供应商确认记录、工单号或外部凭证链接，用于财务核对差异；不直接修改结算金额与结论。
                    </DialogDescription>
                </DialogHeader>
                <DialogBody className="space-y-4">
                    <div className="space-y-1.5">
                        <Label htmlFor="supplier-settlements-evidence-reference">
                            凭证编号或外部链接
                            <RequiredMark />
                        </Label>
                        <Input
                            id="supplier-settlements-evidence-reference"
                            value={referenceId}
                            onChange={(e) =>
                                onReferenceIdChange(e.target.value)
                            }
                            placeholder="如：供应商工单号、对账确认邮件编号或归档文档链接"
                        />
                        <p className="text-xs text-muted-foreground">
                            请填写可追溯的业务凭证编号或共享文档地址，方便复核人查证。
                        </p>
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="supplier-settlements-evidence-comment">
                            业务说明
                        </Label>
                        <Textarea
                            id="supplier-settlements-evidence-comment"
                            value={comment}
                            onChange={(e) => onCommentChange(e.target.value)}
                            placeholder="说明差异核实过程、供应商答复结果或补差约定"
                            rows={3}
                        />
                    </div>
                </DialogBody>
                <DialogFooter>
                    <Button
                        id="supplier-settlements-evidence-cancel"
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <LoadingButton
                        id="supplier-settlements-evidence-confirm"
                        loading={pending}
                        type="button"
                        disabled={pending || !referenceId.trim()}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? "保存中…" : "保存证据"}
                    </LoadingButton>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function SettlementRejectDialog({
    open,
    onOpenChange,
    statement,
    reasonCode,
    onReasonCodeChange,
    comment,
    onCommentChange,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    statement?: SettlementDetailView["statement"]
    reasonCode: string
    onReasonCodeChange: (reasonCode: string) => void
    comment: string
    onCommentChange: (comment: string) => void
    pending: boolean
    onSubmit: () => void | Promise<void>
}) {
    const [commentTouched, setCommentTouched] = React.useState(false)
    React.useEffect(() => {
        if (!open) setCommentTouched(false)
    }, [open])

    const trimmedComment = comment.trim()
    const commentChars = Array.from(trimmedComment).length
    const commentTooLong = commentChars > REJECT_COMMENT_MAX_CHARS
    const commentRequired = reasonCode === "OTHER"
    const commentMissing = commentRequired && trimmedComment.length === 0
    const commentError = commentTooLong
        ? `具体驳回意见不能超过 ${REJECT_COMMENT_MAX_CHARS} 个字（当前 ${commentChars} 字）`
        : commentMissing && commentTouched
          ? "选择「其他」原因时请填写具体驳回意见"
          : undefined
    const confirmDisabled = !reasonCode || commentMissing || commentTooLong

    return (
        <FormalActionConfirmDialog
            layout="compact"
            actionVariant="destructive"
            idPrefix="supplier-settlements-reject"
            open={open}
            onOpenChange={onOpenChange}
            title="驳回复核"
            description="驳回后将退回经办人补充核对并保留本次复核意见记录。"
            actionLabel="驳回复核"
            confirmLabel="确认驳回"
            cancelLabel="取消"
            fromStatus={
                statement
                    ? {
                          label: statement.statusLabel,
                          tone: statement.statusTone,
                      }
                    : { label: "待复核", tone: "warning" }
            }
            toStatus={{ label: "已退回核对", tone: "destructive" }}
            summary={
                statement
                    ? [
                          <span key="no">
                              结算单号：
                              <span className="num font-medium text-foreground">
                                  {statement.statementNo}
                              </span>
                          </span>,
                          <span key="supplier">
                              供应商：{statement.supplierName}
                          </span>,
                      ]
                    : []
            }
            formContent={
                <div className="space-y-3">
                    <div className="space-y-1.5">
                        <Label htmlFor="supplier-settlements-reject-reason">
                            驳回原因
                            <RequiredMark />
                        </Label>
                        <OptionCombobox
                            id="supplier-settlements-reject-reason"
                            value={reasonCode || null}
                            disabled={pending}
                            onValueChange={(v) => onReasonCodeChange(v ?? "")}
                            options={[
                                {
                                    value: "NEEDS_MORE_EVIDENCE",
                                    label: "证据不足",
                                },
                                {
                                    value: "AMOUNT_MISMATCH",
                                    label: "金额仍不一致",
                                },
                                { value: "OTHER", label: "其他" },
                            ]}
                            placeholder="请选择驳回原因"
                            allowClear={false}
                        />
                    </div>
                    <ReasonTextareaField
                        id="supplier-settlements-reject-comment"
                        mode="chars"
                        max={REJECT_COMMENT_MAX_CHARS}
                        label="具体驳回意见"
                        required={commentRequired}
                        value={comment}
                        onChange={onCommentChange}
                        onBlur={() => setCommentTouched(true)}
                        placeholder="说明需要经办人补充核对的明细行、差异或凭证"
                        description={
                            commentRequired
                                ? "选择「其他」原因时必须填写具体驳回意见。"
                                : undefined
                        }
                        rows={3}
                        disabled={pending}
                        error={commentError}
                        quickPhrases={REJECT_QUICK_PHRASES}
                        onCtrlEnterSubmit={() => {
                            setCommentTouched(true)
                            if (confirmDisabled || pending) return
                            void onSubmit()
                        }}
                    />
                </div>
            }
            confirmDisabled={confirmDisabled}
            pending={pending}
            onConfirm={async () => {
                setCommentTouched(true)
                if (confirmDisabled) return
                await onSubmit()
            }}
        />
    )
}

function SettlementSubmitReviewDialog({
    open,
    onOpenChange,
    statement,
    totals,
    reviewerUserId,
    onReviewerUserIdChange,
    pending,
    onConfirm,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    statement: SettlementDetailView["statement"]
    totals: SettlementDetailView["totals"]
    reviewerUserId: string
    onReviewerUserIdChange: (reviewerUserId: string) => void
    pending: boolean
    onConfirm: () => Promise<void>
}) {
    const reviewers = useQuery({
        queryKey: ["supplier-settlement-reviewer-options", statement.id],
        queryFn: () => fetchSettlementReviewerOptions(statement.id),
        enabled: open,
        staleTime: 0,
    })
    const selectedReviewer = reviewers.data?.find(
        (person) => person.user_id === reviewerUserId,
    )
    const previewPayable =
        statement.supplierAmountGross ??
        totals.supplierAmountGross ??
        totals.erpAmountGross

    return (
        <FormalActionConfirmDialog
            actionVariant="default"
            id="supplier-settlements-dialog-submit-review"
            open={open}
            onOpenChange={onOpenChange}
            title="提交复核"
            description="提交后交由所选人员复核，明细与差异结论不可再修改。"
            actionLabel="提交复核"
            confirmLabel="确认提交"
            fromStatus={{
                label: statement.statusLabel,
                tone: statement.statusTone,
            }}
            toStatus={{ label: "待复核", tone: "warning" }}
            summary={[
                <span key="no">
                    结算单号：
                    <span className="num font-medium text-foreground">
                        {statement.statementNo}
                    </span>
                </span>,
                <MoneyWithReadout
                    key="erp"
                    label="ERP 计算金额"
                    value={totals.erpAmountGross}
                />,
                <MoneyWithReadout
                    key="payable"
                    label="本次结算金额"
                    value={previewPayable}
                />,
            ]}
            formContent={
                <div className="space-y-1.5">
                    <Label htmlFor="supplier-settlements-submit-reviewer-input">
                        复核人
                        <RequiredMark />
                    </Label>
                    <OptionCombobox
                        onRefresh={() =>
                            reviewers.refetch({ throwOnError: true })
                        }
                        id="supplier-settlements-submit-reviewer-input"
                        value={reviewerUserId || null}
                        disabled={pending || reviewers.isError}
                        loading={reviewers.isFetching}
                        onValueChange={(value) =>
                            onReviewerUserIdChange(value ?? "")
                        }
                        options={(reviewers.data ?? []).map((person) => ({
                            value: person.user_id,
                            label: `${person.display_name}（${person.account}）`,
                        }))}
                        placeholder="搜索姓名或账号"
                        emptyLabel="暂无可复核人员，请联系管理员配置财务权限"
                    />
                    {reviewers.isError ? (
                        <p role="alert" className="text-xs text-destructive">
                            人员加载失败。
                            <Button
                                id="supplier-settlements-reviewers-retry"
                                variant="link"
                                onClick={() => void reviewers.refetch()}
                            >
                                重试
                            </Button>
                        </p>
                    ) : null}
                </div>
            }
            confirmDisabled={
                !selectedReviewer || reviewers.isFetching || reviewers.isError
            }
            pending={pending}
            onConfirm={onConfirm}
        />
    )
}

function SettlementConfirmSettlementDialog({
    open,
    onOpenChange,
    statement,
    totals,
    pending,
    onConfirm,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    statement: SettlementDetailView["statement"]
    totals: SettlementDetailView["totals"]
    pending: boolean
    onConfirm: () => Promise<void>
}) {
    const payablePreview =
        statement.supplierAmountGross ?? statement.erpAmountGross
    const costDeltaPreview = totals.pendingCostDeltaGross ?? "0.00"

    return (
        <FormalActionConfirmDialog
            actionVariant="default"
            id="supplier-settlements-dialog-confirm-settlement"
            open={open}
            onOpenChange={onOpenChange}
            title="确认结算"
            description="确认后更新成本差额并生成应付，结果不可撤回；经办人不能确认本单。"
            actionLabel="完成结算"
            confirmLabel="确认结算"
            fromStatus={{
                label: statement.statusLabel,
                tone: statement.statusTone,
            }}
            toStatus={{ label: "已确认", tone: "success" }}
            summary={[
                <span key="no">
                    结算单号：
                    <span className="num font-medium text-foreground">
                        {statement.statementNo}
                    </span>
                </span>,
                <MoneyWithReadout
                    key="payable"
                    label="应付金额预览"
                    value={payablePreview}
                />,
                <MoneyWithReadout
                    key="delta"
                    label="成本差额预览"
                    value={costDeltaPreview}
                />,
                <span key="prepared">
                    经办：{statement.preparedBy?.displayName ?? "—"}
                </span>,
            ]}
            effects={["追加成本差额记录", "生成供应商应付"]}
            nextDepartment="供应商往来"
            pending={pending}
            onConfirm={onConfirm}
        />
    )
}

export {
    SettlementConfirmSettlementDialog,
    SettlementEvidenceDialog,
    SettlementRejectDialog,
    SettlementResolveDialog,
    SettlementSubmitReviewDialog,
}
