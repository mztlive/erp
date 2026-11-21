"use client"

import { Input } from "@/components/ui/input"
import { SupplierSearchCombobox } from "@/features/entity-selectors/components/supplier-search-combobox"
import { SettlementPartySearchCombobox } from "@/features/party-selector/settlement-party-search-combobox"

export function PaymentListFilterInput({
    name,
    label,
    value,
    onValueChange,
    ...props
}: {
    id: string
    name: string
    label: string
    value: string
    onValueChange: (value: string) => void
    onBlur: () => void
    placeholder: string
}) {
    if (name === "supplier_id" || name === "payer_party_id") {
        const Selector =
            name === "supplier_id"
                ? SupplierSearchCombobox
                : SettlementPartySearchCombobox
        return (
            <Selector
                {...props}
                purpose="filter"
                value={value || undefined}
                onValueChange={(id) => onValueChange(id ?? "")}
                filterLabel={label}
                {...(name === "payer_party_id" && { label })}
                placeholder="全部"
                allowClear
            />
        )
    }
    return (
        <Input
            {...props}
            value={value}
            onChange={(event) => onValueChange(event.target.value)}
            maxLength={200}
        />
    )
}
