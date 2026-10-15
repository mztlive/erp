"use client"

import { Input } from "@/components/ui/input"
import { SupplierSearchCombobox } from "@/features/entity-selectors/components/supplier-search-combobox"
import { SettlementPartySearchCombobox } from "@/features/party-selector/settlement-party-search-combobox"

export function PaymentListFilterInput({
    name,
    value,
    onValueChange,
    ...props
}: {
    id: string
    name: string
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
                placeholder={
                    name === "supplier_id"
                        ? "搜索并选择供应商"
                        : "搜索并选择我方主体"
                }
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
