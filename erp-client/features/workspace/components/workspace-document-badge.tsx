import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

import { workspaceDocumentBadge } from "../api/work-item-meta"
import type { WorkspaceWorkItem } from "../types"

/**
 * 工作台单据类型徽章。按单据或任务类型显示短标签和色相，便于混排队列扫读。
 */
export function WorkspaceDocumentBadge({
    item,
    decorative = false,
    className,
}: {
    item: Pick<
        WorkspaceWorkItem,
        "workItemType" | "businessObjectType" | "workItemTypeLabel"
    >
    /** 列表行已有 aria-label 时隐藏给读屏，避免重复朗读类型。 */
    decorative?: boolean
    /** 追加到徽章根节点，例如与单号光学对齐。 */
    className?: string
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
            className={cn("shrink-0", className)}
        >
            {badge.label}
        </Badge>
    )
}
