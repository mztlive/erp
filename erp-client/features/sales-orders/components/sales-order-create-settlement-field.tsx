"use client"

import { toFieldErrors } from "@/components/form"
import type { SettlementPartyComboboxItem } from "@/components/business/entity-comboboxes"
import { SettlementPartySearchCombobox } from "@/features/entity-selectors"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
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
                        <span className="text-destructive">*</span>
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
                            ? "默认使用合同的我方签约主体，可按本次销售安排调整。"
                            : "选择本次销售的结算主体，补合同时保留此选择。"}
                    </p>
                    <FieldError
                        errors={toFieldErrors(field.state.meta.errors)}
                    />
                </Field>
            )}
        </form.AppField>
    )
}
