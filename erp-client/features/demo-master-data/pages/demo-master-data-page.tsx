"use client"

import { BusinessFailureState } from "@/components/business"

import * as React from "react"

import { PageScaffold } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { FormalActionConfirmDialog } from "@/components/business/workflow"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import type { DemoCounts } from "@/features/demo-master-data/api"
import {
    useApplyDemoMasterDataMutation,
    useDemoMasterDataStatusQuery,
    useRemoveDemoMasterDataMutation,
} from "@/features/demo-master-data/hooks/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

const COUNT_ROWS: ReadonlyArray<readonly [keyof DemoCounts, string]> = [
    ["customer", "客户"],
    ["supplier", "供应商"],
    ["product", "商品"],
    ["warehouse", "仓库"],
    ["category", "分类"],
    ["brand", "品牌"],
    ["unit", "计量单位"],
]

/**
 * 系统管理中的演示主数据：分批生成固定清单，删除时全库重置。
 */
export function DemoMasterDataPage() {
    const profileQuery = useAccountProfileQuery()
    const statusQuery = useDemoMasterDataStatusQuery()
    const [progress, setProgress] = React.useState<string | null>(null)
    const [confirmRemove, setConfirmRemove] = React.useState(false)
    const apply = useApplyDemoMasterDataMutation((current, total) => {
        setProgress(`正在生成 ${current} / ${total}`)
    })
    const remove = useRemoveDemoMasterDataMutation()
    const canApply = hasPermission(
        profileQuery.data?.permissions,
        "demo_master_data:apply",
    )
    const canRemove =
        profileQuery.data?.account === "admin" &&
        profileQuery.data.role_ids.includes("role-root") &&
        hasPermission(profileQuery.data?.permissions, "demo_master_data:remove")
    const busy = apply.isPending || remove.isPending
    const enabled = statusQuery.data?.enabled === true

    if (statusQuery.isPending) {
        return (
            <PageScaffold density="compact" className={styles.page}>
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="h-40 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="系统"
                title="示例业务数据"
                description="生成示例客户、供应商、商品、仓库、字典及供给，并准备岗位账号、部门、审批流程和默认负责人。新建岗位账号初始密码为 123456，已有账号不改密码。重置系统数据会清空当前系统中的全部业务与示例数据（包括所有单据、审批配置、部门和其他账号），仅保留超管账号及必要权限。重置后不可恢复，可随时重新生成示例业务数据。"
            >
                {canApply ? (
                    <LoadingButton
                        id="demo-master-data-apply"
                        type="button"
                        size="sm"
                        disabled={!enabled || busy}
                        loading={apply.isPending}
                        onClick={() => {
                            remove.reset()
                            setProgress("正在生成")
                            apply.mutate()
                        }}
                    >
                        {apply.isPending
                            ? (progress ?? "正在生成")
                            : "生成示例业务数据"}
                    </LoadingButton>
                ) : null}
                {canRemove ? (
                    <Button
                        id="demo-master-data-remove"
                        type="button"
                        size="sm"
                        variant="destructive"
                        disabled={!enabled || busy}
                        onClick={() => setConfirmRemove(true)}
                    >
                        {remove.isPending
                            ? "正在重置系统数据"
                            : "重置系统数据（仅保留超管账号）"}
                    </Button>
                ) : null}
            </ListWorkspaceHeader>

            {statusQuery.isError && !statusQuery.data ? (
                <BusinessFailureState
                    idPrefix="demo-master-data-status"
                    error={statusQuery.error}
                    title="示例业务数据暂不可查看"
                    onRetry={() => void statusQuery.refetch()}
                />
            ) : null}

            {statusQuery.data && !statusQuery.data.enabled ? (
                <Alert>
                    <AlertTitle>当前环境未启用示例业务数据</AlertTitle>
                    <AlertDescription>
                        当前环境未启用示例数据功能，请联系系统运维人员开启后再试。
                    </AlertDescription>
                </Alert>
            ) : null}

            {statusQuery.data ? (
                <dl className="divide-y divide-border rounded-lg border border-border">
                    {COUNT_ROWS.map(([key, label]) => (
                        <div
                            key={key}
                            className="flex items-center justify-between gap-4 px-4 py-3 text-body-compact"
                        >
                            <dt>{label}</dt>
                            <dd className="text-muted-foreground">
                                已生成 {statusQuery.data.active[key]} / 计划{" "}
                                {statusQuery.data.planned[key]}
                                {statusQuery.data.removed[key] > 0
                                    ? `，${statusQuery.data.removed[key]} 条旧数据待彻底删除`
                                    : ""}
                            </dd>
                        </div>
                    ))}
                </dl>
            ) : null}

            {apply.data && apply.data.length > 0 ? (
                <Alert>
                    <AlertTitle>生成说明</AlertTitle>
                    <AlertDescription>
                        <ul className="list-disc pl-4">
                            {apply.data.map((notice) => (
                                <li key={notice}>{notice}</li>
                            ))}
                        </ul>
                    </AlertDescription>
                </Alert>
            ) : null}

            {remove.isSuccess ? (
                <Alert role="status">
                    <AlertTitle>系统数据已重置</AlertTitle>
                    <AlertDescription>
                        已清理 {remove.data.deleted_documents}{" "}
                        条记录，仅保留超管账号及必要权限。可随时重新生成示例业务数据。
                    </AlertDescription>
                </Alert>
            ) : null}
            <FormalActionConfirmDialog
                idPrefix="demo-master-data-remove"
                open={canRemove && confirmRemove}
                onOpenChange={setConfirmRemove}
                title="重置系统数据（仅保留超管账号）"
                description="此操作会清除系统内的全部示例数据和业务数据。重置前请确认没有其他人员正在操作系统或执行导入导出任务。"
                actionLabel="重置系统数据"
                confirmLabel="确认重置，仅保留超管账号"
                cancelLabel="取消"
                actionVariant="destructive"
                fromStatus="已有数据"
                toStatus="仅保留超管账号"
                summary={["保留超级管理员账号、原密码及全部系统权限。"]}
                irreversibleEffects={[
                    "所有业务单据、基础资料、库存、审批配置与记录、部门、人员权限及其他账号都会永久删除。",
                    "操作日志与单据附件记录将一并清空。",
                    "重置后无法恢复，重新生成示例数据会创建新的岗位账号和业务资料。",
                ]}
                pending={remove.isPending}
                onConfirm={() => {
                    apply.reset()
                    setConfirmRemove(false)
                    remove.mutate()
                }}
            />
        </PageScaffold>
    )
}
