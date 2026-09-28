"use client"

import * as React from "react"

import { PageScaffold } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
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
 * 系统管理中的演示主数据：一次生成固定清单，一次只删除这一批。
 */
export function DemoMasterDataPage() {
    const profileQuery = useAccountProfileQuery()
    const statusQuery = useDemoMasterDataStatusQuery()
    const [progress, setProgress] = React.useState<string | null>(null)
    const [confirmRemove, setConfirmRemove] = React.useState(false)
    const apply = useApplyDemoMasterDataMutation((current, total) => {
        setProgress(`正在生成 ${current} / ${total}`)
    })
    const remove = useRemoveDemoMasterDataMutation(
        (removed, derivedRemoved) => {
            setProgress(
                derivedRemoved > 0
                    ? `已删除主数据 ${removed} 条，关联记录 ${derivedRemoved} 条`
                    : `已删除主数据 ${removed} 条`,
            )
        },
    )
    const canApply = hasPermission(
        profileQuery.data?.permissions,
        "demo_master_data:apply",
    )
    const canRemove = hasPermission(
        profileQuery.data?.permissions,
        "demo_master_data:remove",
    )
    const busy = apply.isPending || remove.isPending
    const enabled = statusQuery.data?.enabled === true
    const activeTotal = totalOf(statusQuery.data?.active)
    const removedTotal = totalOf(statusQuery.data?.removed)

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
                description="生成一批客户、供应商、商品、仓库和字典，并补齐岗位账号、部门、审批流程、默认采购调度人和财务付款、开票负责人。新客户交给销售账号，新供应商交给采购账号。岗位账号已存在时不改密码，新建账号的初始密码是 123456。已有启用的默认责任规则会保留现有负责人。删除时会清掉引用这批客户、供应商或商品的单据、审批和待办，并清掉演示商品自己的库存余额和流水。账号、部门、已发布的审批流程、默认责任规则，以及没有引用这批资料的单据会保留。删除后不可恢复，再次生成会按当前数据模板创建新记录。"
            >
                {canApply ? (
                    <Button
                        id="demo-master-data-apply"
                        type="button"
                        size="sm"
                        disabled={!enabled || busy}
                        onClick={() => {
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
                        disabled={
                            !enabled ||
                            busy ||
                            (activeTotal === 0 && removedTotal === 0)
                        }
                        onClick={() => setConfirmRemove(true)}
                    >
                        {remove.isPending
                            ? (progress ?? "正在删除")
                            : "删除演示主数据"}
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
                    <AlertTitle>删除未完成</AlertTitle>
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

            {confirmRemove ? (
                <AlertDialog
                    open
                    onOpenChange={(open) => !open && setConfirmRemove(false)}
                >
                    <AlertDialogContent>
                        <AlertDialogHeader>
                            <AlertDialogTitle>删除演示主数据</AlertDialogTitle>
                            <AlertDialogDescription>
                                会永久删除这次生成的客户、供应商、商品、仓库和字典，以及引用它们的单据、审批和待办。演示商品自己的库存余额和流水会一并清掉。关联单据混有非演示商品时，会停止删除，请先处理混用单据。岗位账号、部门、已发布的审批流程、默认采购调度人和财务付款、开票负责人，以及没有引用这批资料的单据会保留。删除后不可恢复，再次生成会按当前数据模板创建新记录。
                            </AlertDialogDescription>
                        </AlertDialogHeader>
                        <AlertDialogFooter>
                            <AlertDialogCancel id="demo-master-data-remove-cancel">
                                取消
                            </AlertDialogCancel>
                            <AlertDialogAction
                                id="demo-master-data-remove-confirm"
                                variant="destructive"
                                onClick={() => {
                                    setConfirmRemove(false)
                                    setProgress("正在删除")
                                    remove.mutate()
                                }}
                            >
                                删除
                            </AlertDialogAction>
                        </AlertDialogFooter>
                    </AlertDialogContent>
                </AlertDialog>
            ) : null}
        </PageScaffold>
    )
}

function totalOf(counts: DemoCounts | undefined): number {
    if (!counts) return 0
    return COUNT_ROWS.reduce((sum, [key]) => sum + counts[key], 0)
}
