"use client"

import type { SettlementPartyComboboxItem } from "@/components/business/entity-comboboxes"
import { toFieldErrors } from "@/components/form"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { RequiredMark } from "@/components/ui/required-mark"
import { SettlementPartySearchCombobox } from "@/features/entity-selectors"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"

export function SalesOrderCreateSettlementField({
    form,
    settlementSnapshot,
    hasContract,
}: {
    form: SalesOrderCreateFormApi
    settlementSnapshot?: SettlementPartyComboboxItem
    hasContract: boolean
}) {
    return (
        <form.AppField name="settlementPartyId">
            {(field) => (
                <Field>
                    <FieldLabel htmlFor="sales-orders-create-settlement">
                        结算主体
                        <RequiredMark />
                    </FieldLabel>
                    <SettlementPartySearchCombobox
                        id="sales-orders-create-settlement"
                        purpose="sales-order"
                        selectedSnapshot={settlementSnapshot}
                        value={field.state.value || undefined}
                        onValueChange={(id) => field.handleChange(id ?? "")}
                        onItemChange={(item) =>
                            form.setFieldValue(
                                "settlementEntity",
                                item?.displayName ?? "",
                            )
                        }
                        placeholder="搜索并选择结算主体"
                    />
                    <p className="text-xs text-muted-foreground">
                        {hasContract
                            ? "默认取合同签约主体，可按本单调整"
                            : "后补合同时保留此选择"}
                    </p>
                    <FieldError
                        errors={toFieldErrors(field.state.meta.errors)}
                    />
                </Field>
            )}
        </form.AppField>
    )
}
