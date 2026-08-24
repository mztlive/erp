"use client"

import {
    OptionCombobox,
    type OptionComboboxProps,
} from "@/components/business/option-combobox"
import { useOpaqueReferenceOptionsQuery } from "@/features/supplier-api-connections/hooks/use-opaque-reference-options"
import { getErrorMessage } from "@/lib/api/errors"

export type OpaqueReferenceSearchComboboxProps = Omit<
    OptionComboboxProps,
    "options" | "loading"
> & { kind: "credential" | "endpoint" }

/** 不透明引用选择器；页面不接触引用目录请求，也不接触任何密钥正文。 */
export function OpaqueReferenceSearchCombobox({
    kind,
    emptyLabel,
    ...props
}: OpaqueReferenceSearchComboboxProps) {
    const query = useOpaqueReferenceOptionsQuery(kind)
    return (
        <OptionCombobox
            {...props}
            options={(query.data ?? []).map((option) => ({
                value: option.referenceId,
                label: `${option.alias} · ${option.version}`,
                keywords: option.referenceId,
            }))}
            loading={query.isFetching}
            emptyLabel={
                query.isError
                    ? getErrorMessage(query.error, "引用目录加载失败，请重试")
                    : (emptyLabel ?? "暂时没有可选择的引用资料")
            }
        />
    )
}
