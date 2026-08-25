import { Badge } from "@/components/ui/badge"

import { workspaceDocumentBadge } from "../api/work-item-meta"
import type { WorkspaceWorkItem } from "../types"

/**
 * 工作台单据类型徽章。按单据或任务类型显示短标签和色相，便于混排队列扫读。
 */
export function WorkspaceDocumentBadge({
    item,
    decorative = false,
}: {
    item: Pick<
        WorkspaceWorkItem,
        "workItemType" | "businessObjectType" | "workItemTypeLabel"
    >
    /** 列表行已有 aria-label 时隐藏给读屏，避免重复朗读类型。 */
    decorative?: boolean
}) {
    const badge = workspaceDocumentBadge(
        item.workItemType,
        item.businessObjectType,
        item.workItemTypeLabel,
    )

    return (
        <Badge
            variant={badge.variant}
            aria-hidden={decorative ? true : undefined}
            className="shrink-0"
        >
            {badge.label}
        </Badge>
    )
}
