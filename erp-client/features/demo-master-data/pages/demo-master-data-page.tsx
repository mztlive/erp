"use client"

import * as React from "react"

import { PageScaffold } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { FormalActionConfirmDialog } from "@/components/business/workflow"
import { Button } from "@/components/ui/button"
import type { DemoCounts } from "@/features/demo-master-data/api"
import {
    useApplyDemoMasterDataMutation,
    useDemoMasterDataStatusQuery,
    useRemoveDemoMasterDataMutation,
} from "@/features/demo-master-data/hooks/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api/errors"
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
                title="演示主数据"
                description="生成客户、供应商、商品、仓库、字典及供给，并准备岗位账号、部门、审批流程和默认负责人。新建岗位账号初始密码为 123456，已有账号不改密码。删除演示主数据会清空当前数据库，包括非演示数据、所有单据、审批配置、部门和其他账号，仅保留 admin 账号及必要超管授权。清空后不可恢复，可以重新生成演示数据。"
            >
                {canApply ? (
                    <Button
                        id="demo-master-data-apply"
                        type="button"
                        size="sm"
                        disabled={!enabled || busy}
                        onClick={() => {
                            remove.reset()
                            setProgress("正在生成")
                            apply.mutate()
                        }}
                    >
                        {apply.isPending
                            ? (progress ?? "正在生成")
                            : "生成演示主数据"}
                    </Button>
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
                            ? "正在清空数据库"
                            : "删除演示主数据（清空数据库）"}
                    </Button>
                ) : null}
            </ListWorkspaceHeader>

            {statusQuery.isError ? (
                <Alert variant="destructive" role="alert">
                    <AlertTitle>没能读到演示主数据</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(statusQuery.error, "请稍后重试。")}
                    </AlertDescription>
                </Alert>
            ) : null}

            {statusQuery.data && !statusQuery.data.enabled ? (
                <Alert>
                    <AlertTitle>当前环境未开放演示主数据</AlertTitle>
                    <AlertDescription>
                        请在运行配置里打开 demo.master_data，重新启动后再生成。
                    </AlertDescription>
                </Alert>
            ) : null}

            {statusQuery.data ? (
                <dl className="divide-y divide-border rounded-lg border border-border">
                    {COUNT_ROWS.map(([key, label]) => (
                        <div
                            key={key}
                            className="flex items-center justify-between gap-4 px-4 py-3 text-[13px]"
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

            {apply.error ? (
                <Alert variant="destructive" role="alert">
                    <AlertTitle>生成未完成</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(apply.error, "请稍后重试。")}
                    </AlertDescription>
                </Alert>
            ) : null}
            {remove.error ? (
                <Alert variant="destructive" role="alert">
                    <AlertTitle>未能确认清空结果</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(remove.error, "请稍后重试。")}
                    </AlertDescription>
                </Alert>
            ) : null}
            {apply.data && apply.data.length > 0 ? (
                <Alert>
                    <AlertTitle>有一部分没有生成</AlertTitle>
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
                    <AlertTitle>数据库已清空</AlertTitle>
                    <AlertDescription>
                        已删除 {remove.data.deleted_documents} 条记录，仅保留
                        admin 账号及必要超管授权。可以重新生成演示数据。
                    </AlertDescription>
                </Alert>
            ) : null}
            <FormalActionConfirmDialog
                idPrefix="demo-master-data-remove"
                open={confirmRemove}
                onOpenChange={setConfirmRemove}
                title="清空数据库，仅保留 admin"
                description="此操作会删除当前数据库中的全部演示数据和非演示数据。请先停止其他人员及后台任务的写入。"
                actionLabel="清空数据库"
                confirmLabel="确认清空，仅保留 admin"
                cancelLabel="取消"
                actionVariant="destructive"
                fromStatus="已有数据"
                toStatus="仅保留 admin"
                summary={["保留 admin 账号、原密码和超级管理员权限。"]}
                irreversibleEffects={[
                    "所有业务单据、主数据、库存、审批定义与记录、部门、责任规则和其他账号都会永久删除。",
                    "数据库中的附件资料和审计历史一并清空；文件存储中的附件文件不会删除。",
                    "清空后无法恢复，重新生成会创建新的岗位账号和业务资料。",
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
