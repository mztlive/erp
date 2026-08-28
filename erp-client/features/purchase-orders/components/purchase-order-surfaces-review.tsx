"use client"

import {
    OptionCombobox,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { LinesTable } from "@/features/purchase-orders/components/purchase-order-surfaces-lines-table"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { REJECT_REASON_LABEL } from "@/features/purchase-orders/types"
import { cn } from "@/lib/utils"
import { responsibilityText } from "@/lib/ui-text"

export function ReviewSurface({
    order,
    reviewForm,
    pending,
    canApprove,
    canReject,
    onApprove,
    costMasked,
}: {
    order: NonNullable<ReturnType<typeof usePurchaseOrderCenterQuery>["data"]>
    reviewForm: ReturnType<typeof useAppForm>
    pending: boolean
    canApprove: boolean
    canReject: boolean
    onApprove: () => void
    costMasked: boolean
}) {
    return (
        <Card className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <CardTitle>财务审核视图</CardTitle>
                <CardDescription>
                    以下为采购提交的只读回显，不可修改
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4 pt-4">
                <Alert>
                    <AlertTitle>
                        {order.reviewWorkItem?.processingState ===
                        "APPROVAL_BLOCKED"
                            ? responsibilityText.blocked
                            : canApprove || canReject
                              ? responsibilityText.assignedToMe
                              : responsibilityText.assignedToOther}
                    </AlertTitle>
                    <AlertDescription>
                        {order.reviewWorkItem?.actionBlockers[0]?.message ??
                            (canApprove || canReject
                                ? "当前责任与提交版本均已确认，可提交允许的审核决定。"
                                : "当前页面只读；处理权变化后请刷新。")}
                    </AlertDescription>
                </Alert>

                <Alert>
                    <AlertTitle>本次提交内容</AlertTitle>
                    <AlertDescription>
                        经办 {order.header.submittedBy ?? "—"} · 提交于{" "}
                        {order.header.submittedAt ?? "—"}
                    </AlertDescription>
                </Alert>

                <DescriptionList columns="three">
                    <DescriptionItem>
                        <DescriptionTerm>供应商</DescriptionTerm>
                        <DescriptionDetails>
                            {order.header.supplierSnapshot}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>含税 / 不含税 / 税</DescriptionTerm>
                        <DescriptionDetails className="num">
                            {costMasked
                                ? "•••"
                                : `${order.currentContent.totals.gross} / ${order.currentContent.totals.net} / ${order.currentContent.totals.tax}`}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>付款条件</DescriptionTerm>
                        <DescriptionDetails>
                            {order.header.paymentTermLabel}
                        </DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>

                <LinesTable order={order} costMasked={costMasked} />

                <Separator />

                <div className="flex flex-wrap items-end gap-3">
                    <Button
                        type="button"
                        disabled={pending || !canApprove}
                        onClick={onApprove}
                    >
                        通过
                    </Button>
                </div>

                <form
                    className={cn(surfaceInsetClassName, "space-y-3 p-3")}
                    onSubmit={(event) => {
                        event.preventDefault()
                        void reviewForm.handleSubmit()
                    }}
                >
                    <p className="text-sm font-medium">驳回</p>
                    <div className="space-y-1.5">
                        <Label htmlFor="reject-reason">
                            原因
                            <span className="text-destructive">*</span>
                        </Label>
                        <reviewForm.AppField name="reasonCode">
                            {(field) => (
                                <OptionCombobox
                                    id="reject-reason"
                                    className="w-full"
                                    value={String(field.state.value ?? "")}
                                    onValueChange={(v) =>
                                        field.handleChange(
                                            v ??
                                                String(field.state.value ?? ""),
                                        )
                                    }
                                    options={Object.entries(
                                        REJECT_REASON_LABEL,
                                    ).map(([code, label]) => ({
                                        value: code,
                                        label,
                                    }))}
                                    allowClear={false}
                                    aria-label="原因"
                                    placeholder="选择原因"
                                />
                            )}
                        </reviewForm.AppField>
                    </div>
                    <reviewForm.AppField name="comment">
                        {(field) => (
                            <field.TextareaField
                                label="说明"
                                required
                                placeholder="结构化原因说明"
                                rows={3}
                            />
                        )}
                    </reviewForm.AppField>
                    <reviewForm.AppForm>
                        <reviewForm.SubmitButton
                            label={pending ? "提交中…" : "确认驳回"}
                            disabled={!canReject || pending}
                        />
                    </reviewForm.AppForm>
                </form>
            </CardContent>
        </Card>
    )
}
