"use client"

import * as React from "react"
import { DetailWorkspaceSection } from "@/components/business"
import { withForm } from "@/components/form"
import { Label } from "@/components/ui/label"
import { DatePicker } from "@/components/ui/date-picker"
import { RequiredMark } from "@/components/ui/required-mark"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
import { demandFormDefaults } from "../lib/demand-form"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import type { SalesInquiry } from "../types"

export const InquiryDemandDetails = withForm({
    defaultValues: demandFormDefaults,
    props: {
        inquiry: undefined as SalesInquiry | undefined,
        feeOwner: null as React.ReactNode,
    },
    render: function DemandDetails({ form, inquiry, feeOwner }) {
        return (
            <DetailWorkspaceSection
                title="基本信息"
                width="form"
                divider={false}
                className="py-4 first:pt-0 last:pb-0"
                subtitle={
                    !inquiry ? "销售负责人及业务组织按当前账号确定" : undefined
                }
            >
                <div className="space-y-4">
                    {!inquiry ? (
                        <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_240px]">
                            <form.Field
                                name="customer_id"
                                children={(field) => (
                                    <div className="space-y-2">
                                        <Label htmlFor="sales-inquiry-customer">
                                            客户
                                            <RequiredMark />
                                        </Label>
                                        <CustomerSearchCombobox
                                            id="sales-inquiry-customer"
                                            value={
                                                field.state.value || undefined
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
                                    </div>
                                )}
                            />
                            <form.Field
                                name="deadline_at"
                                children={(field) => (
                                    <div className="space-y-2">
                                        <Label htmlFor="sales-inquiry-deadline">
                                            询价截止日期
                                        </Label>
                                        <DatePicker
                                            id="sales-inquiry-deadline"
                                            size="default"
                                            value={inquiryDateInput(
                                                field.state.value,
                                            )}
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
                        </div>
                    ) : (
                        <div className="grid gap-4 md:grid-cols-[240px_minmax(0,1fr)]">
                            <form.Field
                                name="deadline_at"
                                children={(field) => (
                                    <div className="space-y-2">
                                        <Label htmlFor="sales-inquiry-deadline">
                                            询价截止日期
                                        </Label>
                                        <DatePicker
                                            id="sales-inquiry-deadline"
                                            size="default"
                                            value={inquiryDateInput(
                                                field.state.value,
                                            )}
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
                            <form.AppField
                                name="draft.note"
                                children={(field) => (
                                    <field.TextareaField
                                        id="sales-inquiry-demand-note"
                                        label="需求说明（选填）"
                                        rows={2}
                                        maxLength={4000}
                                        textareaClassName="min-h-16"
                                        placeholder="客户交付要求及需要采购确认的事项"
                                    />
                                )}
                            />
                        </div>
                    )}
                    {!inquiry && (
                        <form.AppField
                            name="draft.note"
                            children={(field) => (
                                <field.TextareaField
                                    id="sales-inquiry-demand-note"
                                    label="需求说明（选填）"
                                    rows={2}
                                    maxLength={4000}
                                    textareaClassName="min-h-16"
                                    placeholder="客户交付要求及需要采购确认的事项"
                                />
                            )}
                        />
                    )}
                    {feeOwner}
                </div>
            </DetailWorkspaceSection>
        )
    },
})
