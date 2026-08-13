"use client"

import { BusinessEmptyState } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { AccessEmptyReason } from "@/features/access-audit/types"

function EmptyByReason({
    reason,
    onClearFilters,
}: {
    reason: AccessEmptyReason
    onClearFilters?: () => void
}) {
    switch (reason) {
        case "NO_MODULE_PERMISSION":
            return (
                <BusinessEmptyState
                    kind="no-scope"
                    title="无模块权限"
                    description="当前账号不能进入「权限与审计」。正常情况下导航入口应隐藏；这与「无数据范围」或「范围内无记录」不同。"
                />
            )
        case "NO_DATA_SCOPE":
            return (
                <BusinessEmptyState
                    kind="no-scope"
                    title="无数据范围"
                    description="你可以进入本页，但当前管理范围内没有任何可配置主体。请查看管理范围或申请授权——不是筛选过严。"
                />
            )
        case "NO_RECORDS_IN_SCOPE":
            return (
                <BusinessEmptyState
                    kind="no-data"
                    title="范围内无记录"
                    description="管理范围有效，但当前视图下没有可展示的记录。可调整时间范围、清除筛选，或（有权时）创建配置。"
                />
            )
        case "FIELD_MASKED":
            return (
                <BusinessEmptyState
                    kind="no-data"
                    title="字段级打码（非空列表）"
                    description="列表与标签保留，敏感值按字段策略打码显示。权限管理员不会因为能配置权限而自动看到业务敏感正文。"
                />
            )
        case "FILTER_NO_RESULT":
        default:
            return (
                <BusinessEmptyState
                    kind="filter"
                    title="当前筛选无结果"
                    description="没有记录符合当前条件。可清除筛选后重试。"
                    action={
                        onClearFilters ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={onClearFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null
                    }
                />
            )
    }
}

export { EmptyByReason }
