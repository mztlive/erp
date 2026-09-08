"use client"

import { useQuery } from "@tanstack/react-query"
import { fetchSettlementReviewerOptions } from "../api/reviewer-options"

import {
    FormalActionConfirmDialog,
    OptionCombobox,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import {
    RESOLUTION_LABEL,
    type DifferenceResolution,
    type SettlementDetailView,
} from "@/features/supplier-settlements/types"

function SettlementResolveDialog({
    open,
    onOpenChange,
    resolution,
    onResolutionChange,
    reasonCode,
    onReasonCodeChange,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    resolution: DifferenceResolution
    onResolutionChange: (resolution: DifferenceResolution) => void
    reasonCode: string
    onReasonCodeChange: (reasonCode: string) => void
    pending: boolean
    onSubmit: () => void
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId="supplier-settlements-resolve-close">
                <DialogHeader>
                    <DialogTitle>登记差异处理结论</DialogTitle>
                    <DialogDescription>
                        登记后将更新待确认成本差额，结论不可撤回；原始证据和历史成本保留。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-3">
                    <div className="space-y-1.5">
                        <Label htmlFor="supplier-settlements-resolve-resolution">
                            处理结论
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
                    <Button
                        id="supplier-settlements-resolve-confirm"
                        type="button"
                        disabled={pending}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? (
                            <Spinner
                                className="size-4 animate-spin"
                                aria-hidden="true"
                            />
                        ) : null}
                        {pending ? "提交中…" : "提交结论"}
                    </Button>
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
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId="supplier-settlements-evidence-close">
                <DialogHeader>
                    <DialogTitle>追加采购协同证据</DialogTitle>
                    <DialogDescription>
                        补充供应商证据或业务说明，不修改结论和金额。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-1.5">
                    <Label htmlFor="supplier-settlements-evidence-reference">
                        凭证编号或链接
                    </Label>
                    <Input
                        id="supplier-settlements-evidence-reference"
                        value={referenceId}
                        onChange={(e) => onReferenceIdChange(e.target.value)}
                        placeholder="例如 ticket://T-123 或 attachment://..."
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="supplier-settlements-evidence-comment">
                        业务说明
                    </Label>
                    <Textarea
                        id="supplier-settlements-evidence-comment"
                        value={comment}
                        onChange={(e) => onCommentChange(e.target.value)}
                        rows={3}
                    />
                </div>
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
                    <Button
                        id="supplier-settlements-evidence-confirm"
                        type="button"
                        disabled={pending || !referenceId.trim()}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? (
                            <Spinner
                                className="size-4 animate-spin"
                                aria-hidden="true"
                            />
                        ) : null}
                        {pending ? "保存中…" : "保存证据"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function SettlementRejectDialog({
    open,
    onOpenChange,
    reasonCode,
    onReasonCodeChange,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    reasonCode: string
    onReasonCodeChange: (reasonCode: string) => void
    pending: boolean
    onSubmit: () => void
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId="supplier-settlements-reject-close">
                <DialogHeader>
                    <DialogTitle>驳回复核</DialogTitle>
                    <DialogDescription>
                        原因必填，退回经办并保留记录。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-1.5">
                    <Label htmlFor="supplier-settlements-reject-reason">
                        处理原因
                    </Label>
                    <OptionCombobox
                        id="supplier-settlements-reject-reason"
                        value={reasonCode || null}
                        onValueChange={(v) => onReasonCodeChange(v ?? "")}
                        options={[
                            { value: "", label: "请选择" },
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
                        placeholder="请选择"
                        allowClear={false}
                    />
                </div>
                <DialogFooter>
                    <Button
                        id="supplier-settlements-reject-cancel"
                        type="button"
                        variant="ghost"
                        disabled={pending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        id="supplier-settlements-reject-confirm"
                        type="button"
                        disabled={!reasonCode || pending}
                        onClick={() => void onSubmit()}
                    >
                        {pending ? (
                            <Spinner
                                className="size-4 animate-spin"
                                aria-hidden="true"
                            />
                        ) : null}
                        {pending ? "提交中…" : "确认驳回"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function SettlementSubmitReviewDialog({
    open,
    onOpenChange,
    statement,
    reviewerUserId,
    onReviewerUserIdChange,
    pending,
    onConfirm,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    statement: SettlementDetailView["statement"]
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
            summary={[statement.statementNo]}

            formContent={
                <div className="space-y-1.5">
                    <Label htmlFor="supplier-settlements-submit-reviewer-input">
                        复核人
                    </Label>
                    <OptionCombobox
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
    return (
        <FormalActionConfirmDialog
            actionVariant="default"
            id="supplier-settlements-dialog-confirm-settlement"
            open={open}
            onOpenChange={onOpenChange}
            title="确认结算"
            description="确认后更新成本差额并生成应付，结果不可撤回；经办人不能确认本单。"
            actionLabel="确认结算"
            confirmLabel="确认结算"
            fromStatus={{
                label: statement.statusLabel,
                tone: statement.statusTone,
            }}
            toStatus={{ label: "已确认", tone: "success" }}
            summary={[
                statement.statementNo,
                `应付金额预览 ${statement.supplierAmountGross ?? statement.erpAmountGross}`,
                `成本差额预览 ${totals.pendingCostDeltaGross ?? "0.00"}`,
                `经办 ${statement.preparedBy?.displayName ?? "—"}`,
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
