"use client"

import {
    OptionCombobox,
    type OptionComboboxProps,
} from "@/components/business/option-combobox"
import { useIntegrationQueueQuery } from "../hooks/queries"
import { getErrorMessage } from "@/lib/api/errors"

export type ReplacementWorkItemSearchComboboxProps = Omit<
    OptionComboboxProps,
    "options" | "loading"
> & { excludeItemId?: string }

/** 重复关闭动作的替代任务选择；候选任务请求和缓存由组件统一持有。 */
export function ReplacementWorkItemSearchCombobox({
    excludeItemId,
    emptyLabel,
    ...props
}: ReplacementWorkItemSearchComboboxProps) {
    const query = useIntegrationQueueQuery({
        view: "mine",
        mode: "errors",
        environment: "all",
        owner: "all",
    })
    const options = (query.data?.items ?? [])
        .filter(
            (item) =>
                item.identity.itemType === "ERROR_TASK" &&
                item.workItem &&
                item.identity.id !== excludeItemId,
        )
        .map((item) => ({
            value: item.workItem!.workItemId,
            label: `${item.identity.number} · ${item.businessObject.title}`,
            keywords: `${item.identity.id} ${item.businessObject.objectId}`,
        }))

    return (
        <OptionCombobox
            {...props}
            options={options}
            loading={query.isFetching}
            emptyLabel={
                query.isError
                    ? getErrorMessage(query.error, "替代任务加载失败，请重试")
                    : emptyLabel
            }
        />
    )
}
