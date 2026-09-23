"use client"

import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { Button } from "@/components/ui/button"
import { WarehouseSearchCombobox } from "@/features/entity-selectors/components/warehouse-search-combobox"
import { SettlementPartySearchCombobox } from "@/features/entity-selectors/components/settlement-party-search-combobox"
import type {
    OrganizationStateView,
    ScopeDimension,
} from "@/features/organization/types"

/** 每种范围只选取对应业务身份；仓库、结算主体不得复用内部组织选项。 */
export function ScopeTargetPicker({
    dimension,
    value,
    onChange,
    units,
}: {
    dimension: ScopeDimension
    value: string[]
    onChange: (ids: string[]) => void
    units: OrganizationStateView["units"]
}) {
    if (dimension === "internal_org") {
        return (
            <MultiOptionCombobox
                id="organization-scope-targets"
                aria-label="组织目标"
                value={value}
                onValueChange={onChange}
                options={units.map((unit) => ({
                    value: unit.id,
                    label: unit.name,
                }))}
                placeholder="选择组织"
            />
        )
    }
    const Picker =
        dimension === "warehouse"
            ? WarehouseSearchCombobox
            : SettlementPartySearchCombobox
    const label = dimension === "warehouse" ? "仓库" : "结算主体"
    return (
        <div className="space-y-2">
            {[...value, ""].map((id, index) => (
                <div className="flex gap-2" key={id || "add"}>
                    <Picker
                        purpose="filter"
                        id={
                            index === 0
                                ? "organization-scope-targets"
                                : `organization-scope-target-${index}`
                        }
                        aria-label={`${label}目标 ${index + 1}`}
                        value={id}
                        onValueChange={(next) => {
                            const ids = value.filter(
                                (_, current) => current !== index,
                            )
                            if (next && !ids.includes(next))
                                ids.splice(index, 0, next)
                            onChange(ids)
                        }}
                        placeholder={`选择${label}`}
                    />
                    {id ? (
                        <Button
                            type="button"
                            variant="ghost"
                            onClick={() =>
                                onChange(value.filter((item) => item !== id))
                            }
                            aria-label={`移除${label}目标 ${index + 1}`}
                        >
                            移除
                        </Button>
                    ) : null}
                </div>
            ))}
        </div>
    )
}
