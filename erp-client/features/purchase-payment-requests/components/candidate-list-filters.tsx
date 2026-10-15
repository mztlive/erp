"use client"

import { useAppForm } from "@/components/form"
import { ListWorkspaceFilterBar } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { PaymentListFilterInput } from "./payment-list-filter-input"
import {
    emptyCandidateFilters,
    candidateFilterFields,
    type CandidateFilters,
} from "../lib/candidate-list-query"

export function CandidateListFilters({
    applied,
    onApply,
}: {
    applied: CandidateFilters
    onApply: (filters: CandidateFilters) => void
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
                ) as CandidateFilters,
            ),
    })
    return (
        <form.Subscribe selector={(state) => state.values}>
            {(values) => (
                <ListWorkspaceFilterBar
                    idPrefix="purchase-payment-candidates-filters"
                    formAriaLabel="可申请采购单查询条件"
                    onSubmit={() => void form.handleSubmit()}
                    search={null}
                    className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:items-end"
                    primaryFilters={candidateFilterFields.map(
                        ([key, label, placeholder]) => (
                            <form.Field key={key} name={key}>
                                {(field) => (
                                    <div className="flex w-52 max-w-full min-w-0 flex-col gap-1.5">
                                        <label
                                            className="text-xs text-muted-foreground"
                                            htmlFor={`purchase-payment-candidates-filter-${key.replaceAll("_", "-")}`}
                                        >
                                            {label}
                                        </label>
                                        <PaymentListFilterInput
                                            id={`purchase-payment-candidates-filter-${key.replaceAll("_", "-")}`}
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
                            id="purchase-payment-candidates-filters-reset"
                            type="button"
                            variant="outline"
                            onClick={() => {
                                form.reset(emptyCandidateFilters)
                                onApply(emptyCandidateFilters)
                            }}
                        >
                            重置
                        </Button>
                    }
                    hasPendingChanges={candidateFilterFields.some(
                        ([key]) => values[key].trim() !== applied[key],
                    )}
                    resultStatus={null}
                    idleHint="仅显示已生效、有可申请余额且你有申请资格的采购单。支持跨页勾选，按供应商和我方主体分别生成申请。"
                />
            )}
        </form.Subscribe>
    )
}
