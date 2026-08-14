"use client"

import { FormalActionConfirmDialog, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
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
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>登记差异处理结论</DialogTitle>
                    <DialogDescription>
                        财务经办追加式结论；不修改左右证据原值或历史成本。结论一经登记不可撤回，将写入审计并改变待确认成本差额。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-3">
                    <div className="space-y-1.5">
                        <Label>受控结论</Label>
                        <OptionCombobox
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
                        <Label>原因码</Label>
                        <OptionCombobox
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
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        type="button"
                        disabled={pending}
                        onClick={() => void onSubmit()}
                    >
                        提交结论
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
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>追加采购协同证据</DialogTitle>
                    <DialogDescription>
                        只追加供应商证据或业务意见和审计，不改变差异结论、试算金额或成本基线。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-1.5">
                    <Label htmlFor="ev-reference">正式证据引用</Label>
                    <Input
                        id="ev-reference"
                        value={referenceId}
                        onChange={(e) => onReferenceIdChange(e.target.value)}
                        placeholder="例如 ticket://T-123 或 attachment://..."
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="ev-comment">业务说明</Label>
                    <Textarea
                        id="ev-comment"
                        value={comment}
                        onChange={(e) => onCommentChange(e.target.value)}
                        rows={3}
                    />
                </div>
                <DialogFooter>
                    <Button
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        type="button"
                        disabled={pending || !referenceId.trim()}
                        onClick={() => void onSubmit()}
                    >
                        保存证据
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
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>驳回复核</DialogTitle>
                    <DialogDescription>
                        原因必填，退回经办并保留记录。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-1.5">
                    <Label>原因码</Label>
                    <OptionCombobox
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
                        type="button"
                        variant="ghost"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        type="button"
                        disabled={!reasonCode || pending}
                        onClick={() => void onSubmit()}
                    >
                        确认驳回
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
    pending,
    onConfirm,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    statement: SettlementDetailView["statement"]
    pending: boolean
    onConfirm: () => Promise<void>
}) {
    return (
        <FormalActionConfirmDialog
            open={open}
            onOpenChange={onOpenChange}
            title="提交复核"
            description="将冻结来源更新时间、明细与差异结论，并创建唯一复核待办。"
            actionLabel="提交复核"
            confirmLabel="确认提交"
            fromStatus={{ label: statement.statusLabel, tone: statement.statusTone }}
            toStatus={{ label: "待复核", tone: "warning" }}
            lockedFields={[
                statement.statementNo,
                "来源数据、明细与差异结论已锁定",
            ]}
            effects={["冻结来源数据与差异结论", "创建结算复核待办"]}
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
            open={open}
            onOpenChange={onOpenChange}
            title="确认结算（不可逆）"
            description="同一次提交追加成本差额、形成唯一应付并锁定处理结果。经办人不可确认本单。"
            actionLabel="确认结算"
            confirmLabel="确认结算"
            fromStatus={{ label: statement.statusLabel, tone: statement.statusTone }}
            toStatus={{ label: "已确认", tone: "success" }}
            lockedFields={[
                statement.statementNo,
                `应付金额预览 ${statement.supplierAmountGross ?? statement.erpAmountGross}`,
                `成本差额预览 ${totals.pendingCostDeltaGross ?? "0.00"}`,
                `经办 ${statement.preparedBy?.displayName ?? "—"}`,
            ]}
            effects={[
                "追加成本差额记录",
                "形成唯一供应商结算应付",
                "锁定处理结果，不可撤回确认",
            ]}
            irreversibleEffects={["确认后付款/进项发票/核销进入供应商往来"]}
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
