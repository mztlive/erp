"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { DetailWorkspaceSection, MoneyValue } from "@/components/business"
import { toFieldErrors, withForm } from "@/components/form"
import { DatePicker } from "@/components/ui/date-picker"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Label } from "@/components/ui/label"
import { RequiredMark } from "@/components/ui/required-mark"
import { StatusBadge } from "@/components/ui/status-badge"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
import { demandFormDefaults } from "../lib/demand-form"
import { summarizeDemand } from "../lib/demand-summary"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import type { SalesInquiry } from "../types"

const FORM_DATE_PICKER_CLASS =
    "w-full [&_[data-variant=control]]:border-border/95 [&_[data-variant=control]]:bg-muted/26 [&_[data-variant=control]]:hover:border-primary/42 [&_[data-variant=control]]:hover:bg-muted/42 [&_[data-variant=control]]:focus-visible:border-primary [&_[data-variant=control]]:focus-visible:bg-card"

export const InquiryDemandDetails = withForm({
    defaultValues: demandFormDefaults,
    props: {
        inquiry: undefined as SalesInquiry | undefined,
        feeOwner: null as React.ReactNode,
    },
    render: function DemandDetails({ form, inquiry, feeOwner }) {
        const values = useStore(form.store, (state) => state.values)
        const summary = React.useMemo(
            () => (inquiry ? null : summarizeDemand(values)),
            [inquiry, values],
        )

        if (!inquiry && summary) {
            return (
                <DetailWorkspaceSection
                    id="sales-inquiry-basic-section"
                    title="基本信息"
                    width="full"
                    divider={false}
                >
                    <div className="grid min-w-0 items-start gap-x-8 gap-y-5 lg:grid-cols-[minmax(0,1fr)_272px] xl:grid-cols-[minmax(0,1fr)_304px]">
                        <div
                            data-slot="detail-form-grid"
                            className="grid min-w-0 max-w-4xl gap-x-5 gap-y-4 sm:grid-cols-12"
                        >
                            <form.Field
                                name="customer_id"
                                validators={{
                                    onSubmit: ({ value }) =>
                                        value ? undefined : "请选择客户",
                                    onChange: ({ value, fieldApi }) =>
                                        fieldApi.form.state.submissionAttempts >
                                            0 && !value
                                            ? "请选择客户"
                                            : undefined,
                                }}
                                children={(field) => {
                                    const errors = toFieldErrors(
                                        field.state.meta.errors,
                                    )
                                    return (
                                        <div className="space-y-1.5 sm:col-span-8">
                                            <Label htmlFor="sales-inquiry-customer">
                                                客户
                                                <RequiredMark />
                                            </Label>
                                            <CustomerSearchCombobox
                                                id="sales-inquiry-customer"
                                                value={
                                                    field.state.value ||
                                                    undefined
                                                }
                                                onValueChange={(id) =>
                                                    field.handleChange(id ?? "")
                                                }
                                                onItemChange={(item) =>
                                                    form.setFieldValue(
                                                        "customer_name",
                                                        item?.legalName ?? "",
                                                    )
                                                }
                                            />
                                            <FieldError
                                                id="sales-inquiry-customer-error"
                                                errors={errors}
                                            />
                                        </div>
                                    )
                                }}
                            />
                            <form.Field
                                name="deadline_at"
                                children={(field) => (
                                    <div className="space-y-1.5 sm:col-span-4">
                                        <Label htmlFor="sales-inquiry-deadline">
                                            询价截止日期
                                        </Label>
                                        <DatePicker
                                            id="sales-inquiry-deadline"
                                            size="default"
                                            placeholder="选择截止日期"
                                            className={FORM_DATE_PICKER_CLASS}
                                            value={
                                                inquiryDateInput(
                                                    field.state.value,
                                                ) || undefined
                                            }
                                            onValueChange={(value) =>
                                                field.handleChange(
                                                    inquiryDateSeconds(
                                                        value ?? "",
                                                    ),
                                                )
                                            }
                                        />
                                    </div>
                                )}
                            />
                            <div className="sm:col-span-12">
                                <form.AppField
                                    name="draft.note"
                                    children={(field) => (
                                        <field.TextareaField
                                            id="sales-inquiry-demand-note"
                                            label="需求说明（选填）"
                                            className="gap-1.5"
                                            rows={2}
                                            maxLength={4000}
                                            textareaClassName="min-h-16"
                                            placeholder="客户交付要求、包装或需要采购确认的事项"
                                        />
                                    )}
                                />
                            </div>
                        </div>

                        <aside
                            aria-label="需求概览"
                            className="min-w-0 border-t border-border/60 pt-4 lg:self-stretch lg:border-t-0 lg:border-l lg:border-border/60 lg:pt-0 lg:pl-7"
                        >
                            <p className="text-xs font-medium text-muted-foreground">
                                需求概览
                            </p>
                            <div className="mt-3 grid grid-cols-2 gap-x-4 gap-y-3.5 sm:grid-cols-3 lg:grid-cols-2">
                                <div className="min-w-0">
                                    <p className="text-xs text-muted-foreground">
                                        询价商品
                                    </p>
                                    <p className="mt-1 flex items-baseline gap-1">
                                        <span className="num text-value-compact font-semibold tracking-tight text-foreground">
                                            {summary.lineCount}
                                        </span>
                                        <span className="text-xs text-muted-foreground">
                                            项
                                        </span>
                                    </p>
                                </div>
                                <div className="min-w-0">
                                    <p className="text-xs text-muted-foreground">
                                        配送资料
                                    </p>
                                    <div className="mt-1.5">
                                        {summary.lineCount === 0 ? (
                                            <span className="text-body-compact text-muted-foreground">
                                                —
                                            </span>
                                        ) : summary.pendingDeliveryCount ===
                                              0 &&
                                          summary.overAllocatedCount === 0 ? (
                                            <StatusBadge
                                                tone="success"
                                                label="全部完善"
                                            />
                                        ) : summary.pendingDeliveryCount > 0 ? (
                                            <StatusBadge
                                                tone="warning"
                                                label={`${summary.pendingDeliveryCount} 项待完善`}
                                            />
                                        ) : (
                                            <StatusBadge
                                                tone="warning"
                                                label={`${summary.overAllocatedCount} 项超量`}
                                            />
                                        )}
                                    </div>
                                </div>
                                <div className="col-span-2 min-w-0 sm:col-span-1 lg:col-span-2">
                                    <div className="flex items-baseline justify-between gap-2">
                                        <p className="text-xs text-muted-foreground">
                                            销售参考合计
                                        </p>
                                        {summary.pricedLineCount > 0 &&
                                        summary.pricedLineCount <
                                            summary.lineCount ? (
                                            <span className="num text-tiny text-muted-foreground">
                                                已填 {summary.pricedLineCount}/
                                                {summary.lineCount} 项
                                            </span>
                                        ) : null}
                                    </div>
                                    <div className="mt-1">
                                        {summary.referenceTotal !== null ? (
                                            <MoneyValue
                                                value={summary.referenceTotal}
                                                size="section"
                                                display="exact"
                                                showHint={false}
                                            />
                                        ) : (
                                            <span className="text-body-compact text-muted-foreground">
                                                —
                                            </span>
                                        )}
                                    </div>
                                </div>
                            </div>
                            <div className="mt-3.5 space-y-1.5 border-t border-border/60 pt-3 text-xs">
                                <div className="flex items-center gap-2">
                                    <span
                                        aria-hidden="true"
                                        className={
                                            values.customer_id
                                                ? "size-1.5 shrink-0 rounded-full bg-success"
                                                : "size-1.5 shrink-0 rounded-full bg-warning"
                                        }
                                    />
                                    <span
                                        className={
                                            values.customer_id
                                                ? "min-w-0 truncate text-foreground"
                                                : "text-muted-foreground"
                                        }
                                        title={
                                            values.customer_name || undefined
                                        }
                                    >
                                        {values.customer_id
                                            ? values.customer_name ||
                                              "已选择客户"
                                            : "未选择客户"}
                                    </span>
                                </div>
                                {summary.invalidQuantityCount > 0 ? (
                                    <div className="flex items-center gap-2 text-destructive">
                                        <span
                                            aria-hidden="true"
                                            className="size-1.5 shrink-0 rounded-full bg-destructive"
                                        />
                                        <span>
                                            <span className="num">
                                                {summary.invalidQuantityCount}
                                            </span>{" "}
                                            项商品数量待修正
                                        </span>
                                    </div>
                                ) : summary.lineCount > 0 &&
                                  summary.missingDueDateCount > 0 ? (
                                    <div className="flex items-center gap-2 text-muted-foreground">
                                        <span
                                            aria-hidden="true"
                                            className="size-1.5 shrink-0 rounded-full bg-muted-foreground/45"
                                        />
                                        <span>
                                            <span className="num">
                                                {summary.missingDueDateCount}
                                            </span>{" "}
                                            项尚未设置交付日
                                        </span>
                                    </div>
                                ) : summary.lineCount > 0 &&
                                  summary.pendingDeliveryCount === 0 ? (
                                    <div className="flex items-center gap-2 text-success-soft-foreground">
                                        <span
                                            aria-hidden="true"
                                            className="size-1.5 shrink-0 rounded-full bg-success"
                                        />
                                        <span>配送资料已完善</span>
                                    </div>
                                ) : null}
                            </div>
                        </aside>
                    </div>
                </DetailWorkspaceSection>
            )
        }

        return (
            <DetailWorkspaceSection
                title="基本信息"
                width="full"
                divider={false}
                className="py-2 first:pt-0 last:pb-0"
            >
                <div className="space-y-3">
                    <div
                        data-slot="detail-form-grid"
                        className="grid items-start gap-x-6 gap-y-4 sm:grid-cols-2 lg:grid-cols-4"
                    >
                        <form.Field
                            name="deadline_at"
                            children={(field) => (
                                <Field className="gap-1.5 lg:col-span-1">
                                    <FieldLabel htmlFor="sales-inquiry-deadline">
                                        询价截止日期
                                    </FieldLabel>
                                    <DatePicker
                                        id="sales-inquiry-deadline"
                                        size="default"
                                        placeholder="选择截止日期"
                                        className={FORM_DATE_PICKER_CLASS}
                                        value={
                                            inquiryDateInput(
                                                field.state.value,
                                            ) || undefined
                                        }
                                        onValueChange={(value) =>
                                            field.handleChange(
                                                inquiryDateSeconds(value ?? ""),
                                            )
                                        }
                                    />
                                </Field>
                            )}
                        />
                        <div className="min-w-0 sm:col-span-1 lg:col-span-3">
                            <form.AppField
                                name="draft.note"
                                children={(field) => (
                                    <field.TextareaField
                                        id="sales-inquiry-demand-note"
                                        label="需求说明（选填）"
                                        className="gap-1.5"
                                        rows={1}
                                        maxLength={4000}
                                        textareaClassName="min-h-control py-1.5 leading-snug"
                                        placeholder="客户交付要求、包装或需要采购确认的事项"
                                    />
                                )}
                            />
                        </div>
                    </div>
                    {feeOwner}
                </div>
            </DetailWorkspaceSection>
        )
    },
})
