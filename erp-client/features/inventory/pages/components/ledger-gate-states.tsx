"use client"

import { BusinessEmptyState, BusinessFailureState, PageHeader, PageScaffold } from "@/components/business"
import { Button } from "@/components/ui/button"

export function InventoryLedgerLoading() {
    return (
        <PageScaffold>
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                {Array.from({ length: 4 }).map((_, i) => (
                    <div
                        key={i}
                        className="h-20 animate-pulse rounded-lg bg-muted"
                    />
                ))}
            </div>
            <div className="h-12 animate-pulse rounded-lg bg-muted" />
            <div className="h-[28rem] animate-pulse rounded-lg bg-muted" />
        </PageScaffold>
    )
}

export function InventoryLedgerPermissionRevoked({
    onRetry,
}: {
    onRetry: () => void
}) {
    return (
        <PageScaffold>
            <PageHeader
                title="库存台账"
                description="模块权限已收回，相关数据已不再展示。"
            />
            <BusinessFailureState
                kind="permission"
                title="权限已收回"
                description="当前账号的库存台账访问权限已被收回。余额、流水、导出结果与展开来源均不可见。"
                action={
                    <Button type="button" onClick={onRetry}>
                        重新检查权限
                    </Button>
                }
            />
        </PageScaffold>
    )
}

export function InventoryLedgerNoScope() {
    return (
        <PageScaffold>
            <PageHeader
                title="库存台账"
                description="有模块权限但未配置仓库数据范围。"
            />
            <BusinessEmptyState
                kind="no-scope"
                title="当前角色未配置仓库数据范围"
                description="不能显示为库存为 0。请联系管理员配置仓库授权后再查询。"
            />
        </PageScaffold>
    )
}
