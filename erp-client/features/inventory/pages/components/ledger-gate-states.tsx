"use client"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"

export function InventoryLedgerLoading() {
    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="库存"
                title="库存台账"
                description="查看账面现存、预占与可用数量。"
            />
            <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
            <div className="h-12 animate-pulse rounded-lg bg-muted" />
            <div className="h-[28rem] animate-pulse bg-muted" />
        </PageScaffold>
    )
}

export function InventoryLedgerPermissionRevoked({
    onRetry,
}: {
    onRetry: () => void
}) {
    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="库存"
                title="库存台账"
                description="模块权限已收回，相关数据已不再展示。"
            />
            <BusinessFailureState
                kind="permission"
                title="权限已收回"
                description="当前账号的库存台账访问权限已被收回。余额、流水、导出结果与展开来源均不可见。"
                action={
                    <Button
                        id="inventory-ledger-permission-retry"
                        type="button"
                        onClick={onRetry}
                    >
                        重新检查权限
                    </Button>
                }
            />
        </PageScaffold>
    )
}

export function InventoryLedgerNoScope() {
    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="库存"
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
