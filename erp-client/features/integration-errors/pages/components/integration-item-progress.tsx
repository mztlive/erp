import {
    ArrowLeftIcon,
    ArrowRightIcon,
    CheckIcon,
    CircleCheckIcon,
    CircleDashedIcon,
    ListChecksIcon,
    ShieldAlertIcon,
    ShieldCheckIcon,
    UsersRoundIcon,
    type LucideIcon,
} from "lucide-react"

import { useWorkspaceTaskPage } from "@/components/business/workspace-task-pane"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { responsibilityText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"
import type { IntegrationResolutionItemView } from "../../types"

const RESPONSIBILITY_STATUS_META: Record<
    ResponsibilityStatus,
    { label: string; tone: StatusTone; icon: LucideIcon }
> = {
    assigned_to_me: {
        label: responsibilityText.assignedToMe,
        tone: "success",
        icon: ShieldCheckIcon,
    },
    assigned_to_other: {
        label: responsibilityText.assignedToOther,
        tone: "neutral",
        icon: UsersRoundIcon,
    },
    blocked: {
        label: responsibilityText.blocked,
        tone: "warning",
        icon: ShieldAlertIcon,
    },
    completed: {
        label: responsibilityText.completed,
        tone: "success",
        icon: CircleCheckIcon,
    },
    closed: {
        label: responsibilityText.closed,
        tone: "neutral",
        icon: CircleDashedIcon,
    },
}

export function IntegrationItemProgress({
    item,
    positionIndex,
    positionTotal,
    responsibilityStatus,
    formalPending,
    focusMode,
    flat,
    onBack,
    onProcess,
    onProcessNext,
    id,
    idPrefix,
}: {
    item: IntegrationResolutionItemView
    positionIndex: number
    positionTotal: number
    responsibilityStatus: ResponsibilityStatus
    formalPending: boolean
    focusMode: boolean
    flat?: boolean
    onBack: () => void
    onProcess: () => void
    onProcessNext: () => void
    id?: string
    idPrefix?: string
}) {
    const fullPage = useWorkspaceTaskPage()
    if (fullPage) return null

    const baseId = idPrefix ?? id ?? "integration-item-progress"
    const responsibility = RESPONSIBILITY_STATUS_META[responsibilityStatus]
    const statusLabel = !item.hasWorkItem
        ? item.identity.itemType === "RECONCILIATION_DIFFERENCE"
            ? "直接对账"
            : "责任未配置"
        : item.workItem?.ownerUser
          ? `当前处理人：${item.workItem.ownerUser.displayName}`
          : responsibility.label
    const canProcess =
        responsibilityStatus === "assigned_to_me" && !formalPending

    return (
        <section
            id={id}
            aria-label="异常处理进度"
            className={cn(
                "flex flex-col gap-3 border-b border-border/60 pb-4 lg:flex-row lg:items-center lg:justify-between",
                flat && "border-t-0",
            )}
        >
            <div
                className="flex min-w-0 flex-wrap items-center gap-2.5"
                aria-live="polite"
            >
                {positionTotal > 0 ? (
                    <StatusBadge
                        tone="info"
                        icon={ListChecksIcon}
                        label={`第 ${positionIndex.toLocaleString("zh-CN")} / ${positionTotal.toLocaleString("zh-CN")} 条`}
                    />
                ) : null}
                <StatusBadge
                    tone={responsibility.tone}
                    icon={responsibility.icon}
                    label={statusLabel}
                />
                {responsibilityStatus === "assigned_to_other" ? (
                    <span className="text-body-compact text-destructive">
                        {responsibilityText.changed}
                    </span>
                ) : null}
            </div>

            <div className="flex flex-wrap items-center gap-2">
                <Button
                    id={`${baseId}-back`}
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={formalPending}
                    onClick={onBack}
                >
                    <ArrowLeftIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    返回队列
                </Button>
                <LoadingButton
                    id={`${baseId}-process`}
                    type="button"
                    size="sm"
                    loading={formalPending}
                    variant={!focusMode ? "secondary" : "default"}
                    disabled={!canProcess}
                    onClick={onProcess}
                >
                    {!formalPending ? (
                        <CheckIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                    ) : null}
                    {formalPending ? "正在处理" : "处理当前"}
                </LoadingButton>
                {!focusMode ? (
                    <LoadingButton
                        id={`${baseId}-process-next`}
                        type="button"
                        size="sm"
                        loading={formalPending}
                        disabled={formalPending}
                        onClick={onProcessNext}
                    >
                        {!formalPending ? (
                            <ArrowRightIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        ) : null}
                        {formalPending ? "正在处理" : "下一项"}
                    </LoadingButton>
                ) : null}
            </div>
        </section>
    )
}
