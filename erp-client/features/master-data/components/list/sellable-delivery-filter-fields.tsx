"use client"

import { OptionCombobox } from "@/components/business"
import {
    ListFilterSection,
    ListWorkspaceFilterField,
} from "@/components/business/list-workspace"
import { Checkbox } from "@/components/ui/checkbox"
import type { useSellableListFilters } from "@/features/master-data/hooks/use-sellable-list-filters"
import { parseSellableDeliveryExtent } from "@/features/master-data/lib/sellable-delivery-filter"
import { DELIVERY_EXTENT_OPTIONS } from "@/lib/delivery-coverage"
import { RegionCascadeSelect } from "@/features/regions"

export type SellableDeliveryFilterControls = Pick<
    ReturnType<typeof useSellableListFilters>,
    | "deliveryRegionDraft"
    | "setDeliveryRegionDraft"
    | "deliveryExtentDraft"
    | "setDeliveryExtentDraft"
    | "freightExtraDraft"
    | "setFreightExtraDraft"
>

/** 公司商品池「更多条件」中的配送地区：地区可选到省、市或区县。 */
export function SellableDeliveryFilterFields({
    idPrefix,
    filters: f,
}: {
    idPrefix: string
    filters: SellableDeliveryFilterControls
}) {
    const disabled = !f.deliveryRegionDraft
    return (
        <ListFilterSection legend="配送地区">
            <ListWorkspaceFilterField
                htmlFor={`${idPrefix}-delivery-region-province`}
                label="能送到"
                className="md:col-span-2"
            >
                <RegionCascadeSelect
                    idPrefix={`${idPrefix}-delivery-region`}
                    ariaLabel="配送地区"
                    value={f.deliveryRegionDraft}
                    onValueChange={f.setDeliveryRegionDraft}
                />
            </ListWorkspaceFilterField>
            <ListWorkspaceFilterField
                htmlFor={`${idPrefix}-delivery-extent`}
                label="范围"
            >
                <OptionCombobox
                    id={`${idPrefix}-delivery-extent`}
                    className="w-full"
                    aria-label="配送范围"
                    value={f.deliveryExtentDraft}
                    onValueChange={(value) =>
                        f.setDeliveryExtentDraft(
                            parseSellableDeliveryExtent(value),
                        )
                    }
                    allowClear={false}
                    disabled={disabled}
                    options={DELIVERY_EXTENT_OPTIONS}
                    placeholder="全部可送"
                />
            </ListWorkspaceFilterField>
            <label
                htmlFor={`${idPrefix}-delivery-freight-extra`}
                className="flex min-h-control items-center gap-2 text-body-compact"
            >
                <Checkbox
                    id={`${idPrefix}-delivery-freight-extra`}
                    checked={f.freightExtraDraft}
                    disabled={disabled}
                    onCheckedChange={(checked) =>
                        f.setFreightExtraDraft(checked === true)
                    }
                />
                包含需另付运费的地区
            </label>
        </ListFilterSection>
    )
}
