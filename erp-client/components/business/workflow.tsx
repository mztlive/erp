"use client"

import * as React from "react"
import {
    CircleCheckIcon,
    Clock3Icon,
    EyeIcon,
    FilterIcon,
    ListChecksIcon,
    LoaderCircleIcon,
    PencilIcon,
    RotateCcwIcon,
    SaveIcon,
    ShieldAlertIcon,
    ShieldCheckIcon,
    TriangleAlertIcon,
    UserRoundIcon,
    UsersRoundIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    DialogTrigger,
} from "@/components/ui/dialog"
import { StatusBadge } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"
import { useControllableDialog } from "@/components/business/workflow-actions"
import type { ControllableDialogProps } from "@/components/business/workflow-actions"

export type BatchImpactPreviewProps = Omit<
    React.ComponentProps<typeof Card>,
    "children" | "title"
> & {
    title?: React.ReactNode
    description?: React.ReactNode
    filterSummary: React.ReactNode
    selectionScope: React.ReactNode
    estimated: number
    processable: number
    skipped: number
    background: boolean
    sensitiveFields?: readonly string[]
    skippedReason?: React.ReactNode
    /** 三个数字的列头文案；默认「预计处理 / 可处理 / 将跳过」。 */
    estimatedLabel?: React.ReactNode
    processableLabel?: React.ReactNode
    skippedLabel?: React.ReactNode
}

/** 批量动作执行前的范围、数量、后台任务和敏感字段预览。 */
function BatchImpactPreview({
    title = "批量操作影响预览",
    description = "执行前请再次核对当前筛选、选择范围和预计处理结果。",
    filterSummary,
    selectionScope,
    estimated,
    processable,
    skipped,
    background,
    sensitiveFields = [],
    skippedReason,
    estimatedLabel = "预计处理",
    processableLabel = "可处理",
    skippedLabel = "将跳过",
    className,
    ...props
}: BatchImpactPreviewProps) {
    return (
        <Card data-slot="batch-impact-preview" className={className} {...props}>
            <CardHeader>
                <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                        <CardTitle className="flex items-center gap-2">
                            <ListChecksIcon
                                aria-hidden="true"
                                className="size-4"
                            />
                            {title}
                        </CardTitle>
                        <CardDescription className="mt-1">
                            {description}
                        </CardDescription>
                    </div>
                    <StatusBadge
                        tone={background ? "info" : "neutral"}
                        icon={background ? Clock3Icon : CircleCheckIcon}
                        label={background ? "将创建后台任务" : "当前页面内执行"}
                    />
                </div>
            </CardHeader>

            <CardContent className="space-y-4">
                <dl className="grid gap-3 sm:grid-cols-2">
                    <div className="rounded-xl border border-border bg-muted p-4">
                        <dt className="flex items-center gap-2 text-sm font-medium text-foreground">
                            <FilterIcon aria-hidden="true" className="size-4" />
                            当前筛选
                        </dt>
                        <dd className="mt-2 text-sm text-muted-foreground">
                            {filterSummary}
                        </dd>
                    </div>
                    <div className="rounded-xl border border-border bg-muted p-4">
                        <dt className="flex items-center gap-2 text-sm font-medium text-foreground">
                            <ShieldCheckIcon
                                aria-hidden="true"
                                className="size-4"
                            />
                            选择范围
                        </dt>
                        <dd className="mt-2 text-sm text-muted-foreground">
                            {selectionScope}
                        </dd>
                    </div>
                </dl>

                <dl className="grid gap-px overflow-hidden rounded-xl border border-border bg-border sm:grid-cols-3">
                    <div className="bg-card p-4">
                        <dt className="flex items-center gap-2 text-sm text-muted-foreground">
                            <ListChecksIcon
                                aria-hidden="true"
                                className="size-4"
                            />
                            {estimatedLabel}
                        </dt>
                        <dd className="num mt-2 text-2xl font-semibold text-foreground">
                            {estimated.toLocaleString("zh-CN")}
                        </dd>
                    </div>
                    <div className="bg-card p-4">
                        <dt className="flex items-center gap-2 text-sm text-muted-foreground">
                            <CircleCheckIcon
                                aria-hidden="true"
                                className="size-4"
                            />
                            {processableLabel}
                        </dt>
                        <dd className="num mt-2 text-2xl font-semibold text-foreground">
                            {processable.toLocaleString("zh-CN")}
                        </dd>
                    </div>
                    <div className="bg-card p-4">
                        <dt className="flex items-center gap-2 text-sm text-muted-foreground">
                            <TriangleAlertIcon
                                aria-hidden="true"
                                className="size-4"
                            />
                            {skippedLabel}
                        </dt>
                        <dd className="num mt-2 text-2xl font-semibold text-foreground">
                            {skipped.toLocaleString("zh-CN")}
                        </dd>
                        {skippedReason != null ? (
                            <div className="mt-2 text-sm text-muted-foreground">
                                {skippedReason}
                            </div>
                        ) : null}
                    </div>
                </dl>

                {sensitiveFields.length > 0 ? (
                    <Alert>
                        <ShieldAlertIcon aria-hidden="true" />
                        <AlertTitle>包含敏感字段</AlertTitle>
                        <AlertDescription>
                            <p>结果将继续执行当前用户的字段权限和遮罩规则。</p>
                            <ul className="mt-2 flex flex-wrap gap-2">
                                {sensitiveFields.map((field) => (
                                    <li key={field}>
                                        <StatusBadge
                                            tone="warning"
                                            icon={ShieldAlertIcon}
                                            label={field}
                                        />
                                    </li>
                                ))}
                            </ul>
                        </AlertDescription>
                    </Alert>
                ) : null}
            </CardContent>
        </Card>
    )
}

export type ConflictAction = "reload" | "save-copy" | "compare"

export type ConflictResolutionDialogProps = ControllableDialogProps & {
    trigger?: React.ReactElement
    title?: string
    description?: React.ReactNode
    currentVersion: React.ReactNode
    localBaseline: React.ReactNode
    actor: React.ReactNode
    changedAt?: React.ReactNode
    diff: React.ReactNode
    pendingAction?: ConflictAction
    onReload: () => void
    onSaveCopy: () => void
    onCompare: () => void
}

/** 并发版本冲突的差异查看与安全处理入口。 */
function ConflictResolutionDialog({
    trigger,
    title = "数据已更新",
    description = "当前数据已经更新，你输入的内容不能直接覆盖。",
    currentVersion,
    localBaseline,
    actor,
    changedAt,
    diff,
    pendingAction,
    onReload,
    onSaveCopy,
    onCompare,
    open,
    defaultOpen,
    onOpenChange,
}: ConflictResolutionDialogProps) {
    const [resolvedOpen, setOpen] = useControllableDialog({
        open,
        defaultOpen,
        onOpenChange,
    })
    const isPending = pendingAction != null

    return (
        <Dialog open={resolvedOpen} onOpenChange={setOpen}>
            {trigger ? <DialogTrigger render={trigger} /> : null}
            <DialogContent className="sm:max-w-2xl">
                <DialogHeader>
                    <DialogTitle className="flex items-center gap-2">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="size-4 text-destructive"
                        />
                        {title}
                    </DialogTitle>
                    <DialogDescription>{description}</DialogDescription>
                </DialogHeader>

                <div className="space-y-4">
                    <dl className="grid gap-3 sm:grid-cols-2">
                        <div className="rounded-xl border border-border bg-muted p-4">
                            <dt className="text-sm font-medium text-muted-foreground">
                                当前系统版本
                            </dt>
                            <dd className="mt-2 flex flex-wrap items-center gap-2">
                                <StatusBadge
                                    tone="success"
                                    icon={CircleCheckIcon}
                                    label="当前有效"
                                />
                                <span className="num font-medium text-foreground">
                                    {currentVersion}
                                </span>
                            </dd>
                        </div>
                        <div className="rounded-xl border border-border bg-muted p-4">
                            <dt className="text-sm font-medium text-muted-foreground">
                                你输入的内容版本
                            </dt>
                            <dd className="mt-2 flex flex-wrap items-center gap-2">
                                <StatusBadge
                                    tone="warning"
                                    icon={Clock3Icon}
                                    label="数据已过期"
                                />
                                <span className="num font-medium text-foreground">
                                    {localBaseline}
                                </span>
                            </dd>
                        </div>
                    </dl>

                    <Alert>
                        <UserRoundIcon aria-hidden="true" />
                        <AlertTitle>当前版本的最近变更</AlertTitle>
                        <AlertDescription>
                            <span className="font-medium text-foreground">
                                {actor}
                            </span>
                            {changedAt != null ? (
                                <span className="num ml-2">{changedAt}</span>
                            ) : null}
                        </AlertDescription>
                    </Alert>

                    <section aria-labelledby="conflict-diff-title">
                        <h3
                            id="conflict-diff-title"
                            className="flex items-center gap-2 text-sm font-medium text-foreground"
                        >
                            <ListChecksIcon
                                aria-hidden="true"
                                className="size-4"
                            />
                            版本差异
                        </h3>
                        <div className="mt-2 rounded-xl border border-border bg-card p-4">
                            {diff}
                        </div>
                    </section>
                </div>

                <DialogFooter>
                    <DialogClose
                        render={<Button variant="ghost" disabled={isPending} />}
                    >
                        取消
                    </DialogClose>
                    <Button
                        type="button"
                        variant="outline"
                        disabled={isPending}
                        onClick={onCompare}
                    >
                        {pendingAction === "compare" ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                                className="animate-spin"
                            />
                        ) : (
                            <EyeIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        查看差异
                    </Button>
                    <Button
                        type="button"
                        variant="secondary"
                        disabled={isPending}
                        onClick={onSaveCopy}
                    >
                        {pendingAction === "save-copy" ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                                className="animate-spin"
                            />
                        ) : (
                            <SaveIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        保留为新草稿
                    </Button>
                    <Button
                        type="button"
                        disabled={isPending}
                        onClick={onReload}
                    >
                        {pendingAction === "reload" ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                                className="animate-spin"
                            />
                        ) : (
                            <RotateCcwIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        重新加载
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

export type EditorPresenceUser = Readonly<{
    id: React.Key
    name: React.ReactNode
}>

export type EditorPresenceProps = Omit<
    React.ComponentProps<"aside">,
    "children"
> & {
    viewers?: readonly EditorPresenceUser[]
    editors?: readonly EditorPresenceUser[]
    reminder?: React.ReactNode
}

function PresenceNames({ users }: { users: readonly EditorPresenceUser[] }) {
    return (
        <>
            {users.map((user, index) => (
                <React.Fragment key={user.id}>
                    {index > 0 ? "、" : null}
                    <span>{user.name}</span>
                </React.Fragment>
            ))}
        </>
    )
}

/** 同一草稿的查看/编辑协作提醒；明确说明它不是业务锁。 */
function EditorPresence({
    viewers = [],
    editors = [],
    reminder = "这是协作提醒；提交仍以系统最新数据为准。",
    className,
    ...props
}: EditorPresenceProps) {
    const hasPresence = viewers.length > 0 || editors.length > 0

    return (
        <aside
            data-slot="editor-presence"
            aria-label="协作状态"
            aria-live="polite"
            className={cn(
                "rounded-2xl border border-border bg-card p-4",
                className,
            )}
            {...props}
        >
            <div className="flex flex-wrap items-center gap-2">
                {editors.length > 0 ? (
                    <StatusBadge
                        tone="warning"
                        icon={PencilIcon}
                        label={`${editors.length.toLocaleString("zh-CN")} 人正在编辑`}
                    />
                ) : null}
                {viewers.length > 0 ? (
                    <StatusBadge
                        tone="info"
                        icon={EyeIcon}
                        label={`${viewers.length.toLocaleString("zh-CN")} 人正在查看`}
                    />
                ) : null}
                {!hasPresence ? (
                    <StatusBadge
                        tone="neutral"
                        icon={UsersRoundIcon}
                        label="当前无其他协作者"
                    />
                ) : null}
            </div>

            {editors.length > 0 ? (
                <p className="mt-3 flex items-start gap-2 text-sm text-foreground">
                    <PencilIcon
                        aria-hidden="true"
                        className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                    />
                    <span>
                        正在编辑：
                        <PresenceNames users={editors} />
                    </span>
                </p>
            ) : null}
            {viewers.length > 0 ? (
                <p className="mt-2 flex items-start gap-2 text-sm text-foreground">
                    <EyeIcon
                        aria-hidden="true"
                        className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                    />
                    <span>
                        正在查看：
                        <PresenceNames users={viewers} />
                    </span>
                </p>
            ) : null}

            <div className="mt-3 flex items-start gap-2 text-sm text-muted-foreground">
                <UserRoundIcon
                    aria-hidden="true"
                    className="mt-0.5 size-4 shrink-0"
                />
                <p>{reminder}</p>
            </div>
        </aside>
    )
}

export { BatchImpactPreview, ConflictResolutionDialog, EditorPresence }

export {
    FormalActionConfirmDialog,
    SequentialProcessBar,
    type FormalActionConfirmDialogProps,
    type ResponsibilityStatus,
    type SequentialProcessBarProps,
    type WorkflowStatus,
} from "@/components/business/workflow-actions"
