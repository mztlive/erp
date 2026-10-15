"use client"

import { useAppForm } from "@/components/form"
import { ListWorkspaceFilterBar } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { PaymentListFilterInput } from "./payment-list-filter-input"
import {
    emptyRequestFilters,
    requestFilterFields,
    type RequestFilters,
} from "../lib/list-query"

export function RequestListFilters({
    applied,
    onApply,
    view,
}: {
    applied: RequestFilters
    onApply: (filters: RequestFilters) => void
    view: "effective" | "draft"
}) {
    const form = useAppForm({
        defaultValues: applied,
        onSubmit: ({ value }) =>
            onApply(
                Object.fromEntries(
                    Object.entries(value).map(([key, text]) => [
                        key,
                        text.trim(),
                    ]),
                ) as RequestFilters,
            ),
    })
    return (
        <form.Subscribe selector={(state) => state.values}>
            {(values) => (
                <ListWorkspaceFilterBar
                    idPrefix="purchase-payment-requests-filters"
                    formAriaLabel="付款申请查询条件"
                    onSubmit={() => void form.handleSubmit()}
                    search={null}
                    className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:items-end"
                    primaryFilters={requestFilterFields.map(
                        ([key, label, placeholder]) => (
                            <form.Field key={key} name={key}>
                                {(field) => (
                                    <div className="flex w-52 max-w-full min-w-0 flex-col gap-1.5">
                                        <label
                                            className="text-xs text-muted-foreground"
                                            htmlFor={`purchase-payment-requests-filter-${key.replaceAll("_", "-")}`}
                                        >
                                            {label}
                                        </label>
                                        <PaymentListFilterInput
                                            id={`purchase-payment-requests-filter-${key.replaceAll("_", "-")}`}
                                            name={key}
                                            value={field.state.value}
                                            onValueChange={field.handleChange}
                                            onBlur={field.handleBlur}
                                            placeholder={placeholder}
                                        />
                                    </div>
                                )}
                            </form.Field>
                        ),
                    )}
                    extraPrimary={
                        <Button
                            id="purchase-payment-requests-filters-reset"
                            type="button"
                            variant="outline"
                            onClick={() => {
                                form.reset(emptyRequestFilters)
                                onApply(emptyRequestFilters)
                            }}
                        >
                            重置
                        </Button>
                    }
                    hasPendingChanges={requestFilterFields.some(
                        ([key]) => values[key].trim() !== applied[key],
                    )}
                    resultStatus={null}
                    idleHint={
                        view === "draft"
                            ? "未提交、已驳回和已撤回的申请，可修改后提交。"
                            : "已提交的申请及付款记录，包含已关闭申请。"
                    }
                />
            )}
        </form.Subscribe>
    )
}
