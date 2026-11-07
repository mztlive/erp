import { DownloadIcon, PlusIcon } from "lucide-react"

import type { PageAction } from "@/components/business/page"
import { masterDataCopy } from "@/features/master-data/lib/copy"

/** 主数据列表页头的「导出」。无结果或导出进行中时不可点。 */
export function masterDataExportAction(input: {
    id: string
    pending: boolean
    rowCount: number
    onClick: () => void
}): PageAction {
    return {
        id: input.id,
        actionKey: "export",
        label: input.pending ? "导出中…" : masterDataCopy.actionExport,
        icon: DownloadIcon,
        variant: "outline",
        loading: input.pending,
        disabled: input.pending || input.rowCount === 0,
        onClick: input.onClick,
    }
}

/** 主数据列表页头的「新建」。无权限时隐藏并给出原因。 */
export function masterDataCreateAction(input: {
    id: string
    canCreate: boolean
    blockedReason?: string
    onClick: () => void
}): PageAction {
    return {
        id: input.id,
        actionKey: "create",
        label: masterDataCopy.actionCreate,
        icon: PlusIcon,
        hidden: !input.canCreate,
        disabled: !input.canCreate,
        title: input.canCreate ? undefined : input.blockedReason,
        onClick: input.onClick,
    }
}
