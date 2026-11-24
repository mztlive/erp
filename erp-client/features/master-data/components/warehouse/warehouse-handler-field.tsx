import type { ComponentProps } from "react"

import { SelectField } from "@/components/form"

const handlerCopy = {
    inbound: {
        label: "入库经办人",
        placeholder: "选择入库经办人",
        emptyLabel: "暂无符合入库资格的账号",
        description: "负责该仓库的采购收货与入库过账。",
    },
    outbound: {
        label: "仓发经办人",
        placeholder: "选择仓发经办人",
        emptyLabel: "暂无符合仓发资格的账号",
        description: "负责该仓库的发货，可与入库经办人为同一人。",
    },
}

export function WarehouseHandlerField({
    duty,
    ...props
}: Pick<
    ComponentProps<typeof SelectField>,
    "id" | "options" | "onRefresh" | "loading" | "disabled"
> & { duty: keyof typeof handlerCopy }) {
    return (
        <SelectField
            {...handlerCopy[duty]}
            {...props}
            allowClear={false}
            required
        />
    )
}
