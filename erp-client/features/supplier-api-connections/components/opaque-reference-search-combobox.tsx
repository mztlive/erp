"use client"

import {
    OptionCombobox,
    type OptionComboboxProps,
} from "@/components/business/option-combobox"
import type { ReferenceOption } from "@/features/supplier-api-connections/api/list"

export type OpaqueReferenceSearchComboboxProps = Omit<
    OptionComboboxProps,
    "options"
> & {
    options: ReferenceOption[]
}

/** 展示服务端配置别名；短时票据仅用于绑定，不展示内部身份或凭据。 */
export function OpaqueReferenceSearchCombobox({
    options,
    ...props
}: OpaqueReferenceSearchComboboxProps) {
    return (
        <OptionCombobox
            {...props}
            options={options.map((option) => ({
                value: option.referenceId,
                label: `${option.alias} · ${option.version}`,
            }))}
        />
    )
}
