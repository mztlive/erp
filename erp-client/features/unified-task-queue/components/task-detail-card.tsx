import { ShieldAlertIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { responsibilityText } from "@/lib/ui-text"

import type { TaskActionKind } from "../hooks/use-task-action"
import { containsAction } from "../lib/responsibility"
import type { QueueWorkItemView } from "../types"

export type TaskDetailCardProps = Readonly<{
    selected: QueueWorkItemView
    handlerHref: string | null
    readonly: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    onAction: (action: TaskActionKind) => void
}>

export function TaskDetailCard({
    selected,
    handlerHref,
    readonly,
    isError,
    error,
    onRetry,
    onAction,
}: TaskDetailCardProps) {
    return (
        <Card>
            <CardHeader>
                <div className="flex flex-wrap items-start justify-between gap-3">
                    <div>
                        <CardTitle>{selected.businessObject}</CardTitle>
                        {selected.workItemTypeLabel !==
                        selected.businessObject ? (
                            <CardDescription>
                                {selected.workItemTypeLabel}
                                {selected.counterparty
                                    ? ` · ${selected.counterparty}`
                                    : ""}
                            </CardDescription>
                        ) : selected.counterparty ? (
                            <CardDescription>
                                {selected.counterparty}
                            </CardDescription>
                        ) : null}
                    </div>
                    <BusinessStatusBadge
                        label={selected.statusPresentation.label}
                        tone={selected.statusPresentation.tone}
                    />
                </div>
            </CardHeader>
            <CardContent className="space-y-4">
                <dl className="grid gap-3 sm:grid-cols-2">
                    {selected.reason ? (
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                为什么需要处理
                            </dt>
                            <dd className="mt-1 text-sm">{selected.reason}</dd>
                        </div>
                    ) : null}
                    {selected.impact ? (
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                业务影响
                            </dt>
                            <dd className="mt-1 text-sm">{selected.impact}</dd>
                        </div>
                    ) : null}
                    {selected.ownerRoleLabel ? (
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                责任角色
                            </dt>
                            <dd className="mt-1 text-sm">
                                {selected.ownerRoleLabel}
                            </dd>
                        </div>
                    ) : null}
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            当前处理人
                        </dt>
                        <dd className="mt-1 text-sm">
                            {selected.responsibilityLabel}
                        </dd>
                    </div>
                </dl>
                {selected.nextActionHint ? (
                    <p className="rounded-md bg-muted/50 px-3 py-2 text-sm">
                        {selected.nextActionHint}
                    </p>
                ) : null}

                {!selected.handlerKnown || !handlerHref ? (
                    <Alert variant="warning">
                        <ShieldAlertIcon aria-hidden="true" />
                        <AlertTitle>当前任务暂不可处理</AlertTitle>
                        <AlertDescription>
                            {selected.actionBlockers[0] ??
                                "任务处理入口尚未配置，请联系系统管理员。"}
                        </AlertDescription>
                    </Alert>
                ) : null}

                {isError ? (
                    <BusinessFailureState
                        error={error}
                        title={responsibilityText.changed}
                        onRetry={onRetry}
                    />
                ) : null}

                {!readonly && selected.status === "OPEN" ? (
                    <>
                        <Separator />
                        <div className="flex flex-wrap gap-2">
                            {containsAction(selected, "RELEASE_TO_TEAM") ? (
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() =>
                                        onAction("RELEASE_TO_TEAM")
                                    }
                                >
                                    {responsibilityText.releaseToTeam}
                                </Button>
                            ) : null}
                            {containsAction(selected, "REASSIGN") ? (
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() => onAction("REASSIGN")}
                                >
                                    {responsibilityText.reassign}
                                </Button>
                            ) : null}
                            {containsAction(selected, "CLOSE") ? (
                                <Button
                                    type="button"
                                    variant="destructive"
                                    onClick={() => onAction("CLOSE")}
                                >
                                    关闭无效任务
                                </Button>
                            ) : null}
                        </div>
                    </>
                ) : null}
            </CardContent>
        </Card>
    )
}
