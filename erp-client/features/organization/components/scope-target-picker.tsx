"use client"

import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { toAutomationIdSegment } from "@/lib/automation-id"
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
    id = "organization-scope-targets",
    disabled = false,
    value,
    onChange,
    units,
}: {
    id?: string
    disabled?: boolean
    dimension: ScopeDimension
    value: string[]
    onChange: (ids: string[]) => void
    units: OrganizationStateView["units"]
}) {
    if (dimension === "internal_org") {
        return (
            <MultiOptionCombobox
                id={id}
                disabled={disabled}
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
            {[...value, ""].map((targetId, index) => (
                <div className="flex gap-2" key={targetId || "add"}>
                    <Picker
                        purpose="filter"
                        id={
                            targetId
                                ? `${id}-${toAutomationIdSegment(targetId)}`
                                : id
                        }
                        disabled={disabled}
                        aria-label={`${label}目标 ${index + 1}`}
                        value={targetId}
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
                    {targetId ? (
                        <Button
                            id={`${id}-remove-${toAutomationIdSegment(targetId)}`}
                            disabled={disabled}
                            type="button"
                            variant="ghost"
                            onClick={() =>
                                onChange(
                                    value.filter((item) => item !== targetId),
                                )
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
