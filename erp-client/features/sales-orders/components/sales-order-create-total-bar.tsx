"use client"

import * as React from "react"
import { useSelector } from "@tanstack/react-form"
import { MapPinIcon, TriangleAlertIcon } from "lucide-react"

import {
    MoneyValue,
    ValidationSummary,
    detailWorkspaceStyles,
} from "@/components/business"
import { FormError, toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import {
    calculateGrossProfit,
    calculateTotals,
} from "@/features/sales-orders/lib/sales-order-create-model"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { summarizeOrderDelivery } from "../lib/sales-delivery-pool"

const HEADER_VALIDATION_FIELDS = [
    { name: "contractId", label: "有效合同", targetId: "contractId" },
    {
        name: "customerId",
        label: "客户",
        targetId: "sales-orders-create-customer",
    },
    {
        name: "evidenceAttachments",
        label: "开单凭证",
        targetId: "sales-orders-create-evidence-select",
    },
    {
        name: "ownerName",
        label: "负责销售",
        targetId: "sales-orders-create-header-owner-name",
    },
    {
        name: "welfareScene",
        label: "福利场景",
        targetId: "sales-orders-create-header-welfare-scene",
    },
    {
        name: "paymentTerms",
        label: "付款条件",
        targetId: "sales-orders-create-header-payment-terms",
    },
    {
        name: "fulfillmentDeadline",
        label: "履约期限",
        targetId: "sales-orders-create-header-fulfillment-deadline",
    },
    {
        name: "receivableDueDate",
        label: "应收到期日",
        targetId: "sales-orders-create-header-receivable-due-date",
    },
    {
        name: "taxRatePercent",
        label: "税率",
        targetId: "sales-orders-create-header-tax-rate",
    },
    { name: "customerName", label: "客户", targetId: "contractId" },
    {
        name: "settlementEntity",
        label: "结算主体",
        targetId: "sales-orders-create-settlement",
    },
    {
        name: "settlementPartyId",
        label: "结算主体",
        targetId: "sales-orders-create-settlement",
    },
] as const

export type SalesOrderCreateTotalBarProps = {
    form: SalesOrderCreateFormApi
    isSubmitting: boolean
    onSaveDraftClick: () => void
    onSubmitClick: () => void
    onOpenDeliveryOverview?: () => void
    onLocateIncompleteLines?: () => void
    onExpandHeader?: () => void
}

export function SalesOrderCreateTotalBar({
    form,
    isSubmitting,
    onSaveDraftClick,
    onSubmitClick,
    onOpenDeliveryOverview,
    onLocateIncompleteLines,
    onExpandHeader,
}: SalesOrderCreateTotalBarProps) {
    /** 提交失败后汇总单据头错误，避免只拦提交却看不到原因。 */
    const headerIssues = useSelector(form.store, (state) => {
        if (state.submissionAttempts === 0) return []
        return HEADER_VALIDATION_FIELDS.flatMap((field) => {
            const meta = state.fieldMeta[field.name]
            return toFieldErrors(meta?.errors ?? [])
                .filter((error) => Boolean(error?.message))
                .map((error, index) => ({
                    id: `${field.name}-${index}`,
                    label: field.label,
                    message: error!.message!,
                    targetId: field.targetId,
                }))
        })
    })

    /** 提交失败后汇总跨页商品明细错误数量，支持一键切到待完善筛选视图。 */
    const invalidLineCount = useSelector(form.store, (state) => {
        if (state.submissionAttempts === 0) return 0
        const invalidIndexes = new Set<string>()
        for (const [key, meta] of Object.entries(state.fieldMeta)) {
            const match = key.match(/^lineItems\[(\d+)\]\./)
            if (!match) continue
            const errors = toFieldErrors(meta?.errors ?? []).filter((error) =>
                Boolean(error?.message),
            )
            if (errors.length > 0) {
                invalidIndexes.add(match[1]!)
            }
        }
        return invalidIndexes.size
    })

    // 若头部被收起且提交触发了基本信息校验错误，自动展开便于直接修正。
    React.useEffect(() => {
        if (headerIssues.length > 0) {
            onExpandHeader?.()
        }
    }, [headerIssues.length, onExpandHeader])

    return (
        <>
            {headerIssues.length > 0 ? (
                <div
                    className="border-t border-border/60 py-3"
                    onClickCapture={() => onExpandHeader?.()}
                >
                    <ValidationSummary
                        issues={headerIssues}
                        title={`基本信息共 ${headerIssues.length} 项待处理`}
                    />
                </div>
            ) : null}

            {invalidLineCount > 0 && onLocateIncompleteLines ? (
                <div
                    role="alert"
                    className="flex flex-wrap items-center justify-between gap-2 border-t border-destructive-border/70 bg-destructive-soft/60 px-3.5 py-2 text-body-compact text-destructive-soft-foreground"
                >
                    <span className="inline-flex items-center gap-1.5">
                        <TriangleAlertIcon
                            className="size-4 shrink-0"
                            aria-hidden="true"
                        />
                        <span>
                            销售明细共有{" "}
                            <span className="num font-semibold">
                                {invalidLineCount}
                            </span>{" "}
                            项商品存在未填写或待修正字段（含当前分页以外的商品）。
                        </span>
                    </span>
                    <Button
                        id="sales-orders-create-locate-incomplete-lines"
                        type="button"
                        variant="outline"
                        size="xs"
                        onClick={onLocateIncompleteLines}
                    >
                        定位待完善商品
                    </Button>
                </div>
            ) : null}

            <form.AppForm>
                <FormError
                    id="sales-orders-create-submit-error"
                    tabIndex={0}
                    className="max-h-32 min-w-0 shrink-0 overflow-y-auto border-t border-border/60 py-3 wrap-anywhere"
                />
            </form.AppForm>

            <form.Subscribe selector={(state) => state.values}>
                {(values) => {
                    const totals = calculateTotals(
                        values.lineItems,
                        values.taxRatePercent,
                    )
                    const skuCount = values.lineItems.filter((line) =>
                        line.sku.trim(),
                    ).length
                    const delivery =
                        values.nature === "physical_service" &&
                        values.lineItems.length > 0
                            ? summarizeOrderDelivery(values.lineItems)
                            : null
                    const grossProfit = calculateGrossProfit(values.lineItems)
                    const grossProfitSign =
                        grossProfit === null
                            ? 0
                            : compareDecimal(grossProfit, "0", 2)
                    // 负毛利红色、正毛利绿色，零毛利保持默认色。
                    const grossProfitTone =
                        grossProfitSign < 0
                            ? "text-destructive font-medium"
                            : grossProfitSign > 0
                              ? "text-success-soft-foreground font-medium"
                              : "text-foreground"
                    return (
                        <aside
                            data-slot="sticky-total-bar"
                            aria-label="金额汇总与操作"
                            className="sticky bottom-0 z-20 mt-auto shrink-0 border-t border-border/70 bg-card/95 py-3 backdrop-blur-xs"
                        >
                            <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
                                <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
                                    <span className="num text-body-compact font-medium text-foreground">
                                        共 {skuCount} 项商品
                                    </span>
                                    {delivery && onOpenDeliveryOverview ? (
                                        <Button
                                            id="sales-orders-create-total-bar-delivery"
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className={cn(
                                                "h-6 gap-1 px-1.5 text-xs",
                                                delivery.incompleteLineCount > 0
                                                    ? "text-warning-soft-foreground hover:bg-warning-soft/70"
                                                    : detailWorkspaceStyles.quietButton,
                                            )}
                                            onClick={onOpenDeliveryOverview}
                                        >
                                            <MapPinIcon
                                                className="size-3"
                                                aria-hidden="true"
                                            />
                                            <span className="num">
                                                收货{" "}
                                                {delivery.completeLineCount}/
                                                {delivery.totalLines}
                                                {delivery.uniqueAddressCount > 0
                                                    ? `（${delivery.uniqueAddressCount} 地址）`
                                                    : ""}
                                            </span>
                                        </Button>
                                    ) : null}
                                    <span
                                        aria-hidden="true"
                                        className="hidden h-3.5 w-px bg-border sm:inline-block"
                                    />
                                    <span>
                                        不含税 <MoneyValue value={totals.net} />
                                    </span>
                                    <span>
                                        税额 <MoneyValue value={totals.tax} />
                                        <span className="num">
                                            （{values.taxRatePercent || "0"}%）
                                        </span>
                                    </span>
                                    {values.nature === "physical_service" ? (
                                        <span title="各行（含税成交单价 − 采购供给单价）× 数量，逐行舍入后合计">
                                            预估含税毛利{" "}
                                            {grossProfit === null ? (
                                                <span>待完善</span>
                                            ) : (
                                                <span
                                                    className={cn(
                                                        grossProfitTone,
                                                    )}
                                                >
                                                    <MoneyValue
                                                        value={grossProfit}
                                                        inheritColor
                                                    />
                                                </span>
                                            )}
                                        </span>
                                    ) : null}
                                </div>

                                <div className="flex flex-wrap items-center justify-between gap-4 lg:justify-end">
                                    <div className="flex items-baseline gap-2.5">
                                        <span className="text-xs text-muted-foreground">
                                            含税合计
                                        </span>
                                        <MoneyValue
                                            value={totals.gross}
                                            size="section"
                                        />
                                    </div>
                                    <span
                                        aria-hidden="true"
                                        className="hidden h-6 w-px bg-border/80 sm:inline-block"
                                    />
                                    <div className="flex shrink-0 items-center gap-2">
                                        <form.AppForm>
                                            <form.SubmitButton
                                                id="sales-orders-create-save-draft"
                                                showError={false}
                                                variant="outline"
                                                label={
                                                    isSubmitting
                                                        ? "处理中…"
                                                        : "保存草稿"
                                                }
                                                pendingLabel="正在保存草稿…"
                                                loading={isSubmitting}
                                                disabled={isSubmitting}
                                                onClick={onSaveDraftClick}
                                            />
                                            <form.SubmitButton
                                                id="sales-orders-create-submit"
                                                showError={false}
                                                data-testid="sales-order-submit"
                                                label="提交审批"
                                                pendingLabel="正在提交…"
                                                loading={isSubmitting}
                                                disabled={isSubmitting}
                                                onClick={onSubmitClick}
                                            >
                                                {isSubmitting
                                                    ? "处理中…"
                                                    : "提交审批"}
                                            </form.SubmitButton>
                                        </form.AppForm>
                                    </div>
                                </div>
                            </div>
                        </aside>
                    )
                }}
            </form.Subscribe>
        </>
    )
}
