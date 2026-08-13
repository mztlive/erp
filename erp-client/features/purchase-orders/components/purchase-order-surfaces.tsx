"use client"

import type * as React from "react"

import {
    MoneyValue,
    OptionCombobox,
    QuantityValue,
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
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PAYMENT_TERM_OPTIONS,
    PURCHASE_TYPE_LABEL,
    REJECT_REASON_LABEL,
} from "@/features/purchase-orders/types"
import {
    positiveDecimal,
    taxRateValid,
} from "@/features/purchase-orders/lib/purchase-order-validation"
import { cn } from "@/lib/utils"

export function LinesTable({
    order,
    costMasked,
}: {
    order: NonNullable<ReturnType<typeof usePurchaseOrderCenterQuery>["data"]>
    costMasked: boolean
}) {
    return (
        <div className="overflow-hidden rounded-lg ring-1 ring-foreground/[0.04]">
            <Table data-density="compact">
                <TableHeader>
                    <TableRow>
                        <TableHead>项目</TableHead>
                        <TableHead>类型</TableHead>
                        <TableHead data-align="end">数量</TableHead>
                        <TableHead data-align="end">含税单价</TableHead>
                        <TableHead data-align="end">税率</TableHead>
                        <TableHead data-align="end">交期</TableHead>
                        <TableHead data-align="end">行含税</TableHead>
                        <TableHead data-align="end">税额</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {order.currentContent.lines.map((line) => (
                        <TableRow key={line.lineId}>
                            <TableCell className="max-w-[16rem] whitespace-normal">
                                <div className="font-medium">
                                    {line.itemName}
                                </div>
                                {line.procurementConfirmationLineId ? (
                                    <div className="text-tiny text-muted-foreground">
                                        {line.salesAllocationLabel ??
                                            `确认分行 · ${line.itemName}`}
                                    </div>
                                ) : null}
                            </TableCell>
                            <TableCell className="text-xs text-muted-foreground">
                                {line.lineType === "LOGISTICS_FEE"
                                    ? "物流费用"
                                    : "商品/服务"}
                            </TableCell>
                            <TableCell data-align="end">
                                {line.lineType === "LOGISTICS_FEE" ? (
                                    "—"
                                ) : (
                                    <QuantityValue
                                        value={line.quantity ?? "0"}
                                        unit={line.unit}
                                    />
                                )}
                            </TableCell>
                            <TableCell data-align="end">
                                {costMasked ? (
                                    "•••"
                                ) : (
                                    <MoneyValue value={line.unitCostGross} />
                                )}
                            </TableCell>
                            <TableCell data-align="end" className="num text-xs">
                                {(Number(line.inputTaxRate) * 100).toFixed(0)}%
                            </TableCell>
                            <TableCell data-align="end" className="num text-xs">
                                {line.expectedDeliveryDate ?? "—"}
                            </TableCell>
                            <TableCell data-align="end">
                                {costMasked ? (
                                    "•••"
                                ) : (
                                    <MoneyValue
                                        value={line.grossAmount}
                                        taxBasis="gross"
                                    />
                                )}
                            </TableCell>
                            <TableCell data-align="end">
                                {costMasked ? (
                                    "•••"
                                ) : (
                                    <MoneyValue value={line.taxAmount} />
                                )}
                            </TableCell>
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
        </div>
    )
}

export function EditSurface({
    order,
    draftForm,
    lineEdits,
    setLineEdits,
    draftEditToken,
    canSubmit,
    savePending,
    onSave,
    onSubmitOpen,
}: {
    order: NonNullable<ReturnType<typeof usePurchaseOrderCenterQuery>["data"]>
    draftForm: ReturnType<typeof useAppForm>
    lineEdits: Record<
        string,
        { quantity?: string; unitCostGross?: string; inputTaxRate: string }
    >
    setLineEdits: React.Dispatch<
        React.SetStateAction<
            Record<
                string,
                {
                    quantity?: string
                    unitCostGross?: string
                    inputTaxRate: string
                }
            >
        >
    >
    draftEditToken: string | null
    canSubmit: boolean
    savePending: boolean
    onSave: () => void
    onSubmitOpen: () => void
}) {
    return (
        <Card className={surfacePanelClassName}>
            <CardHeader className="border-b border-border/30">
                <CardTitle>
                    {order.identity.reviewStatus === "REJECTED"
                        ? "被驳回待修改"
                        : "采购草稿"}
                </CardTitle>
                <CardDescription>
                    来源销售 {order.header.salesOrderNo}
                    {order.header.creationBasisId
                        ? ` · 来自采购二次确认（销售单 ${order.header.salesOrderNo}）`
                        : ""}
                    。⌘S 保存 · ⌘↵
                    打开提交确认。拆单维度（供应商、类型、付款条件、履约责任）已固定，不能修改。
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4 pt-4">
                {!draftEditToken ? (
                    <Alert variant="warning">
                        <AlertTitle>正在进入编辑…</AlertTitle>
                        <AlertDescription>
                            编辑内容仅保存在当前页面；刷新或关闭将丢失，请及时保存。
                        </AlertDescription>
                    </Alert>
                ) : (
                    <p className="text-xs text-muted-foreground">正在编辑中</p>
                )}

                <div className="grid gap-3 sm:grid-cols-2">
                    <div className="space-y-1.5">
                        <Label>供应商（只读）</Label>
                        <div
                            className={cn(
                                surfaceInsetClassName,
                                "px-3 py-2 text-sm",
                            )}
                        >
                            {order.header.supplierSnapshot}
                        </div>
                    </div>
                    <div className="space-y-1.5">
                        <Label>采购类型 / 履约责任（只读）</Label>
                        <div
                            className={cn(
                                surfaceInsetClassName,
                                "px-3 py-2 text-sm",
                            )}
                        >
                            {PURCHASE_TYPE_LABEL[order.header.purchaseType]} ·{" "}
                            {
                                FULFILLMENT_RESPONSIBILITY_LABEL[
                                    order.header.fulfillmentResponsibility
                                ]
                            }
                        </div>
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                        <Label htmlFor="payment-term">付款条件</Label>
                        <draftForm.AppField name="paymentTermCode">
                            {(field) => (
                                <OptionCombobox
                                    id="payment-term"
                                    className="w-full"
                                    value={String(field.state.value ?? "")}
                                    onValueChange={(v) =>
                                        field.handleChange(
                                            v ??
                                                String(field.state.value ?? ""),
                                        )
                                    }
                                    options={
                                        PAYMENT_TERM_OPTIONS.some(
                                            (option) =>
                                                option.label ===
                                                order.header.paymentTermLabel,
                                        )
                                            ? [...PAYMENT_TERM_OPTIONS]
                                            : [
                                                  {
                                                      value: order.header
                                                          .paymentTermCode,
                                                      label: order.header
                                                          .paymentTermLabel,
                                                  },
                                                  ...PAYMENT_TERM_OPTIONS,
                                              ]
                                    }
                                    allowClear={false}
                                    aria-label="付款条件"
                                    placeholder="付款条件"
                                />
                            )}
                        </draftForm.AppField>
                    </div>
                </div>

                <Separator />

                <div className="space-y-2">
                    <h3 className="text-sm font-semibold">明细（系统计算）</h3>
                    <div className="overflow-hidden rounded-lg ring-1 ring-foreground/[0.04]">
                        <Table data-density="compact">
                            <TableHeader>
                                <TableRow>
                                    <TableHead>项目</TableHead>
                                    <TableHead data-align="end">数量</TableHead>
                                    <TableHead data-align="end">
                                        含税单价
                                    </TableHead>
                                    <TableHead data-align="end">税率</TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {order.currentContent.lines.map((line) => (
                                    <TableRow key={line.lineId}>
                                        <TableCell className="whitespace-normal">
                                            <div className="font-medium">
                                                {line.itemName}
                                            </div>
                                            <div className="text-tiny text-muted-foreground">
                                                {line.lineType ===
                                                "LOGISTICS_FEE"
                                                    ? "物流费用"
                                                    : line.procurementConfirmationLineId
                                                      ? (line.salesAllocationLabel ??
                                                        `确认分行 · ${line.itemName}`)
                                                      : "商品/服务"}
                                            </div>
                                        </TableCell>
                                        <TableCell data-align="end">
                                            {line.lineType ===
                                            "LOGISTICS_FEE" ? (
                                                "—"
                                            ) : (
                                                <>
                                                    <input
                                                        className="num w-20 rounded border border-border bg-background px-2 py-1 text-right text-sm"
                                                        value={
                                                            lineEdits[
                                                                line.lineId
                                                            ]?.quantity ?? ""
                                                        }
                                                        onChange={(event) =>
                                                            setLineEdits(
                                                                (prev) => ({
                                                                    ...prev,
                                                                    [line.lineId]:
                                                                        {
                                                                            ...prev[
                                                                                line
                                                                                    .lineId
                                                                            ],
                                                                            inputTaxRate:
                                                                                prev[
                                                                                    line
                                                                                        .lineId
                                                                                ]
                                                                                    ?.inputTaxRate ??
                                                                                line.inputTaxRate,
                                                                            quantity:
                                                                                event
                                                                                    .target
                                                                                    .value,
                                                                        },
                                                                }),
                                                            )
                                                        }
                                                        aria-label={`${line.itemName} 数量`}
                                                    />
                                                    {!positiveDecimal(
                                                        lineEdits[line.lineId]
                                                            ?.quantity ??
                                                            line.quantity,
                                                    ) ? (
                                                        <span className="block text-tiny text-destructive">
                                                            须为正数
                                                        </span>
                                                    ) : null}
                                                </>
                                            )}
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <>
                                                <input
                                                    className="num w-28 rounded border border-border bg-background px-2 py-1 text-right text-sm"
                                                    value={
                                                        lineEdits[line.lineId]
                                                            ?.unitCostGross ??
                                                        ""
                                                    }
                                                    onChange={(event) =>
                                                        setLineEdits(
                                                            (prev) => ({
                                                                ...prev,
                                                                [line.lineId]: {
                                                                    ...prev[
                                                                        line
                                                                            .lineId
                                                                    ],
                                                                    inputTaxRate:
                                                                        prev[
                                                                            line
                                                                                .lineId
                                                                        ]
                                                                            ?.inputTaxRate ??
                                                                        line.inputTaxRate,
                                                                    unitCostGross:
                                                                        event
                                                                            .target
                                                                            .value,
                                                                },
                                                            }),
                                                        )
                                                    }
                                                    aria-label={`${line.itemName} 含税单价`}
                                                />
                                                {!positiveDecimal(
                                                    lineEdits[line.lineId]
                                                        ?.unitCostGross ??
                                                        line.unitCostGross,
                                                ) ? (
                                                    <span className="block text-tiny text-destructive">
                                                        须为正数
                                                    </span>
                                                ) : null}
                                            </>
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <>
                                                <input
                                                    className="num w-20 rounded border border-border bg-background px-2 py-1 text-right text-sm"
                                                    value={(() => {
                                                        const raw =
                                                            lineEdits[
                                                                line.lineId
                                                            ]?.inputTaxRate ??
                                                            line.inputTaxRate
                                                        if (raw === "")
                                                            return ""
                                                        const value =
                                                            Number(raw)
                                                        return Number.isFinite(
                                                            value,
                                                        )
                                                            ? String(
                                                                  value * 100,
                                                              )
                                                            : raw
                                                    })()}
                                                    onChange={(event) => {
                                                        const raw =
                                                            event.target.value
                                                        const parsed =
                                                            Number(raw)
                                                        setLineEdits(
                                                            (prev) => ({
                                                                ...prev,
                                                                [line.lineId]: {
                                                                    ...prev[
                                                                        line
                                                                            .lineId
                                                                    ],
                                                                    inputTaxRate:
                                                                        raw ===
                                                                            "" ||
                                                                        !Number.isFinite(
                                                                            parsed,
                                                                        )
                                                                            ? raw
                                                                            : String(
                                                                                  parsed /
                                                                                      100,
                                                                              ),
                                                                },
                                                            }),
                                                        )
                                                    }}
                                                    aria-label={`${line.itemName} 税率（%）`}
                                                />
                                                <span className="ml-1 text-xs text-muted-foreground">
                                                    %
                                                </span>
                                                {!taxRateValid(
                                                    lineEdits[line.lineId]
                                                        ?.inputTaxRate ??
                                                        line.inputTaxRate,
                                                ) ? (
                                                    <span className="block text-tiny text-destructive">
                                                        税率须为 0-1 的小数（如
                                                        0.13）
                                                    </span>
                                                ) : null}
                                            </>
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                </div>

                <div className="flex flex-wrap gap-2">
                    <Button
                        type="button"
                        variant="outline"
                        disabled={!draftEditToken || savePending}
                        onClick={onSave}
                    >
                        {savePending ? "保存中…" : "保存草稿"}
                    </Button>
                    <Button
                        type="button"
                        disabled={!draftEditToken || !canSubmit || savePending}
                        onClick={onSubmitOpen}
                    >
                        提交财务审核
                    </Button>
                </div>
            </CardContent>
        </Card>
    )
}

export function ReviewSurface({
    order,
    reviewForm,
    pending,
    onApprove,
    costMasked,
}: {
    order: NonNullable<ReturnType<typeof usePurchaseOrderCenterQuery>["data"]>
    reviewForm: ReturnType<typeof useAppForm>
    pending: boolean
    onApprove: () => void
    costMasked: boolean
}) {
    return (
        <Card className={surfacePanelClassName}>
            <CardHeader className="border-b border-border/30">
                <CardTitle>财务审核视图</CardTitle>
                <CardDescription>
                    以下为采购提交的只读回显，不可修改
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4 pt-4">
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
                        disabled={pending || !order.reviewWorkItem}
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
                        <Label htmlFor="reject-reason">原因</Label>
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
                                placeholder="结构化原因说明"
                                rows={3}
                            />
                        )}
                    </reviewForm.AppField>
                    <reviewForm.AppForm>
                        <reviewForm.SubmitButton
                            label={pending ? "提交中…" : "确认驳回"}
                            disabled={!order.reviewWorkItem}
                        />
                    </reviewForm.AppForm>
                </form>
            </CardContent>
        </Card>
    )
}
