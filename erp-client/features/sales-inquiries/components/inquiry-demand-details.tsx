"use client"

import * as React from "react"
import { ChevronDownIcon } from "lucide-react"
import { withForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
import { demandFormDefaults } from "../lib/demand-form"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import type { SalesInquiry } from "../types"

export const InquiryDemandDetails = withForm({
    defaultValues: demandFormDefaults,
    props: { inquiry: undefined as SalesInquiry | undefined },
    render: function DemandDetails({ form, inquiry }) {
        const [open, setOpen] = React.useState(!inquiry)
        return (
            <Collapsible
                open={open}
                onOpenChange={setOpen}
                className={
                    inquiry ? "space-y-3" : "rounded-lg bg-muted/40 px-5 py-4"
                }
            >
                <div className="flex items-start justify-between gap-4">
                    <div className="min-w-0 flex-1 self-center">
                        {inquiry ? (
                            <form.Subscribe
                                selector={(state) => state.values.draft.note}
                            >
                                {(note) => (
                                    <p className="flex gap-2 text-sm text-muted-foreground">
                                        <span className="shrink-0">
                                            需求说明
                                        </span>
                                        <span
                                            className="min-w-0 truncate"
                                            title={note || undefined}
                                        >
                                            {note || "未填写"}
                                        </span>
                                    </p>
                                )}
                            </form.Subscribe>
                        ) : (
                            <h2 className="text-sm font-semibold">基本信息</h2>
                        )}
                    </div>
                    <CollapsibleTrigger
                        id="sales-inquiry-basic-toggle"
                        render={
                            <Button
                                type="button"
                                variant={inquiry ? "ghost" : "outline"}
                                size="sm"
                                className="shrink-0"
                            />
                        }
                    >
                        {open ? "收起" : inquiry ? "编辑说明" : "展开编辑"}
                        <ChevronDownIcon
                            aria-hidden="true"
                            className="size-4 transition-transform in-data-[panel-open]:rotate-180"
                        />
                    </CollapsibleTrigger>
                </div>
                <CollapsibleContent keepMounted>
                    <div
                        className={
                            inquiry
                                ? "space-y-4"
                                : "mt-4 space-y-4 border-t pt-4"
                        }
                    >
                        {!inquiry && (
                            <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_260px]">
                                <form.Field
                                    name="customer_id"
                                    children={(field) => (
                                        <div className="space-y-2">
                                            <Label htmlFor="sales-inquiry-customer">
                                                客户{" "}
                                                <span className="text-destructive">
                                                    *
                                                </span>
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
                                <p className="text-xs text-muted-foreground md:col-span-2">
                                    销售负责人及业务组织按当前账号确定。
                                </p>
                            </div>
                        )}
                        <form.AppField
                            name="draft.note"
                            children={(field) => (
                                <field.TextareaField
                                    id="sales-inquiry-demand-note"
                                    label="需求说明（选填）"
                                    rows={2}
                                    textareaClassName="min-h-16"
                                    placeholder="客户交付要求及需要采购确认的事项"
                                />
                            )}
                        />
                    </div>
                </CollapsibleContent>
            </Collapsible>
        )
    },
})
