import { useState } from "react"
import { ChevronDownIcon } from "lucide-react"

import { OptionCombobox } from "@/components/business"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { LoadingButton } from "@/components/ui/loading-button"
import { mapWorkItemDto, useWorkItemsQuery } from "@/features/work-items"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    WORK_ITEM_CLOSE_COMMENT_MAX_CHARS,
    utf8ByteLength,
} from "@/lib/validation/reason-length"

import { INTEGRATION_ACTION_LABEL } from "../../../lib/presentation"
import type { IntegrationResolutionItemView } from "../../../types"
import {
    isInvestigationClose,
    isInvestigationConclusion,
    type InvestigationAction,
    type InvestigationCommands,
} from "./commands"

function labelFor(action: InvestigationAction) {
    if (action === "RESOLVE") return "完成处理"
    return INTEGRATION_ACTION_LABEL[action]
}

function ReplacementTaskField({
    id,
    item,
    commands,
}: {
    id: string
    item: IntegrationResolutionItemView
    commands: InvestigationCommands
}) {
    const [search, setSearch] = useState("")
    const workItem = item.workItem
    const candidates = useWorkItemsQuery({
        scope: "managed",
        workItemType: workItem?.workItemType,
        query: search.trim() || undefined,
        timezone: "Asia/Shanghai",
        pageSize: 30,
    })
    const objectType =
        item.identity.itemType === "ERROR_TASK"
            ? "integration_error_task"
            : "reconciliation_difference"
    const options = (candidates.isError ? [] : (candidates.data?.items ?? []))
        .map(mapWorkItemDto)
        .filter(
            (candidate) =>
                candidate.status === "OPEN" &&
                !candidate.approvalStepInstanceId &&
                candidate.businessObjectType === objectType &&
                candidate.workItemType === workItem?.workItemType &&
                candidate.workItemId !== workItem?.workItemId &&
                Boolean(candidate.businessObjectLabel),
        )
        .map((candidate) => ({
            label: candidate.businessObjectLabel,
            value: candidate.workItemId,
        }))

    return (
        <OptionCombobox
            id={id}
            value={commands.replacementTaskId || null}
            options={options}
            onValueChange={(value) =>
                commands.setReplacementTaskId(value ?? "")
            }
            aria-label="替代任务"
            placeholder="搜索同类型的开放任务"
            filterMode="remote"
            retainMissingSelection
            onSearchChange={setSearch}
            loading={candidates.isFetching}
            emptyLabel={candidates.isError ? "替代任务加载失败" : undefined}
            disabled={commands.locked}
            className="w-full max-w-72"
            size="sm"
            required
        />
    )
}

function focusFormField(id: string) {
    window.requestAnimationFrame(() => {
        document.getElementById(id)?.focus()
    })
}

export function InvestigationActionForm({
    item,
    idPrefix,
    commands,
}: {
    item: IntegrationResolutionItemView
    idPrefix: string
    commands: InvestigationCommands
}) {
    const { actions, confirmAction, locked, showComment } = commands
    if (!actions.length || (!confirmAction && !showComment)) return null
    if (confirmAction && !actions.includes(confirmAction)) return null
    const concluding = confirmAction && isInvestigationConclusion(confirmAction)
    const closing = confirmAction && isInvestigationClose(confirmAction)

    return (
        <section
            aria-labelledby={`${idPrefix}-action-heading`}
            className="min-w-0 space-y-2.5 border-t border-border/60 pt-3.5"
        >
            <h2
                id={`${idPrefix}-action-heading`}
                className="text-section-title font-semibold"
            >
                {confirmAction ? labelFor(confirmAction) : "处理说明"}
            </h2>
            {closing ? (
                <p className="text-xs text-muted-foreground">
                    关闭后当前任务退出待处理队列，业务记录保持原状。
                </p>
            ) : null}
            {concluding ? (
                <>
                    <p className="text-xs text-muted-foreground">
                        对账结论将写入处理记录。请核对差异原因及关联凭证。
                    </p>
                    <OptionCombobox
                        id={`${idPrefix}-reconciliation-reason`}
                        value={commands.reasonId || null}
                        options={
                            item.reconciliationReasonRegistry?.registeredReasons
                                .filter(
                                    (reason) =>
                                        reason.conclusion === confirmAction,
                                )
                                .map((reason) => ({
                                    value: reason.registeredReasonId,
                                    label: reason.label,
                                })) ?? []
                        }
                        onValueChange={(value) =>
                            commands.setReasonId(value ?? "")
                        }
                        aria-label="差异原因"
                        placeholder="选择差异原因"
                        allowClear={false}
                        disabled={locked}
                        className="w-full max-w-72"
                        size="sm"
                    />
                </>
            ) : null}
            {confirmAction === "CLOSE_DUPLICATE" ? (
                <ReplacementTaskField
                    id={`${idPrefix}-replacement-task`}
                    item={item}
                    commands={commands}
                />
            ) : null}
            <ReasonTextareaField
                id={`${idPrefix}-remark`}
                mode={closing ? "chars" : "utf8-bytes"}
                max={
                    closing
                        ? WORK_ITEM_CLOSE_COMMENT_MAX_CHARS
                        : BPM_REASON_MAX_UTF8_BYTES
                }
                label={
                    confirmAction === "CLOSE_MISROUTED"
                        ? "关闭说明"
                        : closing
                          ? "关闭说明（可选）"
                          : "处理说明（可选）"
                }
                required={confirmAction === "CLOSE_MISROUTED"}
                error={
                    closing
                        ? Array.from(commands.comment.trim()).length >
                          WORK_ITEM_CLOSE_COMMENT_MAX_CHARS
                            ? `关闭说明最多 ${WORK_ITEM_CLOSE_COMMENT_MAX_CHARS} 字`
                            : undefined
                        : utf8ByteLength(commands.comment) >
                            BPM_REASON_MAX_UTF8_BYTES
                          ? `处理说明超出 ${BPM_REASON_MAX_UTF8_BYTES} 字节上限`
                          : undefined
                }
                value={commands.comment}
                onChange={commands.setComment}
                rows={2}
                disabled={locked}
                onCtrlEnterSubmit={
                    confirmAction && commands.canSubmit(confirmAction)
                        ? () => void commands.execute(confirmAction)
                        : undefined
                }
            />
            {concluding &&
            commands.reasonId &&
            !commands.canSubmit(confirmAction) &&
            !locked ? (
                <p className="text-xs text-muted-foreground">
                    提交前请选择与结论匹配的原因，并核对所需凭证。
                </p>
            ) : null}
        </section>
    )
}

export function InvestigationFeedback({
    idPrefix,
    item,
    commands,
}: {
    idPrefix: string
    item: IntegrationResolutionItemView
    commands: InvestigationCommands
}) {
    const { state } = commands
    if (state.status === "unknown") {
        return (
            <div
                role="alert"
                className="space-y-1 rounded-r-md border-l-2 border-l-warning bg-warning-soft px-3 py-2.5 text-xs wrap-anywhere text-warning-soft-foreground"
            >
                <p className="text-body-compact font-semibold">
                    提交结果待核对
                </p>
                <p>
                    {commands.canRecover
                        ? `「${labelFor(state.action)}」的提交内容已锁定。点击「核对提交结果」确认原操作。`
                        : "本次提交内容已锁定，请由具备权限的责任人核对结果。"}
                </p>
            </div>
        )
    }
    if (state.status === "conflict" || state.status === "failed") {
        return (
            <div
                role="alert"
                className="flex flex-wrap items-center gap-2 rounded-r-md border-l-2 border-l-destructive bg-destructive-soft px-3 py-2.5 text-xs wrap-anywhere text-destructive-soft-foreground"
            >
                <p className="min-w-0 flex-1">{state.message}</p>
                {state.status === "conflict" ? (
                    <Button
                        id={`${idPrefix}-refresh-conflict`}
                        type="button"
                        variant="outline"
                        size="xs"
                        onClick={() => void commands.refresh()}
                    >
                        刷新最新数据
                    </Button>
                ) : null}
            </div>
        )
    }
    if (state.status === "settled") {
        return (
            <div
                role="status"
                className="space-y-1 border-l-2 border-l-border pl-3 text-xs wrap-anywhere"
            >
                <p className="text-body-compact font-medium">
                    {state.result.title}
                </p>
                <p className="text-muted-foreground">
                    {state.result.description}
                </p>
            </div>
        )
    }

    const blockers = item.actionBlockers.filter(
        (blocker) =>
            commands.actions.some((action) => action === blocker.action) ||
            (blocker.action === "PROCESS" && commands.actions.length > 0),
    )
    return blockers.length ? (
        <ul className="space-y-1 text-xs text-muted-foreground">
            {blockers.map((blocker) => (
                <li
                    key={`${blocker.action}-${blocker.code}`}
                    className="wrap-anywhere"
                >
                    {blocker.message}
                </li>
            ))}
        </ul>
    ) : null
}

export function InvestigationFooter({
    item,
    idPrefix,
    commands,
}: {
    item: IntegrationResolutionItemView
    idPrefix: string
    commands: InvestigationCommands
}) {
    const { actions, state, locked, confirmAction } = commands
    const availableConfirm =
        confirmAction && actions.includes(confirmAction)
            ? confirmAction
            : undefined
    const primary = actions.includes("RESOLVE")
        ? "RESOLVE"
        : actions.includes("QUERY_ORIGINAL_RESULT")
          ? "QUERY_ORIGINAL_RESULT"
          : (actions.find((action) => !isInvestigationClose(action)) ??
            actions[0])
    const active = availableConfirm ?? primary
    const secondary = actions.filter((action) => action !== primary)
    const needsConfirmation = (action: InvestigationAction) =>
        isInvestigationClose(action) ||
        isInvestigationConclusion(action) ||
        action === "RESOLVE"

    function select(action: InvestigationAction) {
        if (!needsConfirmation(action)) {
            void commands.execute(action)
            return
        }
        commands.setConfirmAction(action)
        if (isInvestigationConclusion(action)) {
            const options =
                item.reconciliationReasonRegistry?.registeredReasons ?? []
            commands.setReasonId(
                options.find((reason) => reason.conclusion === action)
                    ?.registeredReasonId ?? "",
            )
        }
        focusFormField(`${idPrefix}-remark`)
    }

    const status = (
        <p
            role="status"
            className="mr-auto min-w-0 text-xs text-muted-foreground wrap-anywhere"
        >
            {state.status === "pending"
                ? "正在提交 · 内容已锁定"
                : state.status === "unknown"
                  ? "提交待核对 · 内容已锁定"
                  : state.status === "settled" && state.result.terminal
                    ? state.result.title
                    : actions.length
                      ? item.status.label
                      : `${item.status.label} · 只读`}
        </p>
    )

    const buttons = locked ? (
        state.status === "unknown" && !commands.canRecover ? null : (
            <LoadingButton
                id={`${idPrefix}-recover`}
                variant="default"
                type="button"
                size="sm"
                loading={state.status === "pending"}
                disabled={!commands.canRecover}
                onClick={() => {
                    if (state.status === "unknown")
                        void commands.execute(state.action, true)
                }}
            >
                {state.status === "pending" ? "提交中" : "核对提交结果"}
            </LoadingButton>
        )
    ) : active ? (
        <>
            {availableConfirm || commands.showComment ? (
                <Button
                    id={`${idPrefix}-cancel-action`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                        commands.setConfirmAction(undefined)
                        commands.setShowComment(false)
                    }}
                >
                    取消
                </Button>
            ) : (
                <DropdownMenu>
                    <DropdownMenuTrigger
                        id={`${idPrefix}-other-actions`}
                        render={
                            <Button type="button" variant="ghost" size="sm" />
                        }
                        disabled={state.status === "conflict"}
                    >
                        更多操作
                        <ChevronDownIcon aria-hidden="true" />
                    </DropdownMenuTrigger>
                    <DropdownMenuContent align="end" side="top">
                        <DropdownMenuItem
                            id={`${idPrefix}-edit-remark`}
                            onClick={() => {
                                commands.setShowComment(true)
                                focusFormField(`${idPrefix}-remark`)
                            }}
                        >
                            填写处理说明
                        </DropdownMenuItem>
                        {secondary.map((action) => (
                            <DropdownMenuItem
                                key={action}
                                id={`${idPrefix}-action-${toAutomationIdSegment(action)}`}
                                disabled={
                                    !needsConfirmation(action) &&
                                    !commands.canSubmit(action)
                                }
                                onClick={() => select(action)}
                            >
                                {labelFor(action)}
                            </DropdownMenuItem>
                        ))}
                    </DropdownMenuContent>
                </DropdownMenu>
            )}
            <Button
                id={`${idPrefix}-submit-${toAutomationIdSegment(active)}`}
                type="button"
                variant="default"
                size="sm"
                disabled={
                    state.status === "conflict" ||
                    ((availableConfirm || !needsConfirmation(active)) &&
                        !commands.canSubmit(active))
                }
                onClick={() => {
                    if (availableConfirm)
                        void commands.execute(availableConfirm)
                    else select(active)
                }}
            >
                {availableConfirm
                    ? `确认${labelFor(active).replace(/^确认/u, "")}`
                    : labelFor(active)}
            </Button>
        </>
    ) : null

    return (
        <WorkspaceTaskFooter
            summary={status}
            fallback={
                <>
                    {status}
                    {buttons}
                </>
            }
        >
            {buttons}
        </WorkspaceTaskFooter>
    )
}
