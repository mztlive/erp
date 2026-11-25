"use client"

import * as React from "react"
import { FilePenLineIcon, UserRoundCogIcon } from "lucide-react"
import { useStore } from "@tanstack/react-form"
import {
    FormalActionConfirmDialog,
    MultiOptionCombobox,
    detailWorkspaceStyles,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { LoadingButton } from "@/components/ui/loading-button"
import { toast } from "@/components/ui/toast"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { getErrorMessage } from "@/lib/api/errors"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"
import { inquiryPath } from "../api"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import {
    INQUIRY_STATUS_LABEL,
    INQUIRY_STATUS_TONE,
    type ProcurementInquiry,
    type SalesInquiry,
} from "../types"
import type {
    InquiryPublishContext,
    InquiryTask,
} from "./inquiry-feedback-editor"

type InquiryAction =
    | "submit"
    | "accept"
    | "reinquire"
    | "close"
    | "supplement"
    | "publish"
    | "renew"
    | "reassign"

const ACTION_LABEL: Record<InquiryAction, string> = {
    submit: "提交采购",
    accept: "接受本轮结果",
    reinquire: "要求重新询价",
    close: "关闭询价",
    supplement: "要求补充需求",
    publish: "发布本轮反馈",
    renew: "发起新一轮报价",
    reassign: "改派责任采购",
}

export type InquiryActionsRenderSlots = {
    primaryAction: React.ReactNode
    secondaryActions: React.ReactNode
    assignmentActions: React.ReactNode
    alerts: React.ReactNode
}

export function InquiryActions({
    inquiry,
    procurement,
    task,
    userId,
    permissions,
    onSaved,
    disabled = false,
    disabledReason,
    renderActions,
    workspace = false,
    fillHeight = false,
    feedbackRound,
    feedbackDirty = false,
    validateBeforePublish,
    beforePublish,
    onPublishingChange,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    procurement: boolean
    task?: InquiryTask
    userId: string
    permissions: readonly string[]
    onSaved: () => void
    disabled?: boolean
    disabledReason?: string
    feedbackRound?: number
    feedbackDirty?: boolean
    validateBeforePublish?: () => boolean
    beforePublish?: () => Promise<InquiryPublishContext | null>
    onPublishingChange?: (publishing: boolean) => void
    workspace?: boolean
    fillHeight?: boolean
    renderActions?: (slots: InquiryActionsRenderSlots) => React.ReactNode
}) {
    const [action, setAction] = React.useState<InquiryAction | null>(null)
    const [error, setError] = React.useState<string | null>(null)
    const submitting = React.useRef(false)
    const submitRequiresReason = inquiry.requirement_version > 0
    const requiresReason = (item: InquiryAction) =>
        item === "submit"
            ? submitRequiresReason
            : [
                  "reinquire",
                  "close",
                  "supplement",
                  "reassign",
                  "renew",
              ].includes(item)
    const command = useInquiryCommand(userId, inquiry.id)
    const storageKey = `erp.sales-inquiry-action:${encodeURIComponent(userId)}:${encodeURIComponent(inquiry.id)}`
    const form = useAppForm({
        defaultValues: {
            reason: "",
            concerns: "",
            line_ids: [] as string[],
            fee_ids: [] as string[],
            buyer_user_id: "",
        },
        onSubmitMeta: null as InquiryAction | null,
        listeners: {
            onChange: ({ formApi }) => {
                try {
                    localStorage.setItem(
                        storageKey,
                        JSON.stringify(formApi.state.values),
                    )
                } catch {
                    /* 提交身份另由命令账本持久化。 */
                }
            },
        },
        onSubmit: async ({ value, meta: action }) => {
            if (
                !action ||
                disabled ||
                submitting.current ||
                (feedbackDirty && action !== "publish")
            )
                return
            submitting.current = true
            setError(null)
            let feedbackSaved = false
            try {
                if (
                    (action === "accept" || action === "reinquire") &&
                    feedbackRound != null &&
                    feedbackRound !== inquiry.feedback_round
                )
                    throw new Error("请切回最新一轮反馈后再接受或重新询价。")
                const taskBody = task
                    ? { task_id: task.id, task_version: task.version }
                    : {}
                const data: Record<string, unknown> = {
                    expected_version: inquiry.version,
                }
                if (requiresReason(action) && !value.reason.trim())
                    throw new Error("请填写本次操作原因。")
                if (action === "submit")
                    data.reason = submitRequiresReason
                        ? value.reason.trim()
                        : ""
                if (action === "accept")
                    data.feedback_round = inquiry.feedback_round
                if (action === "reinquire")
                    data.data = {
                        reason: value.reason.trim(),
                        line_ids: value.line_ids,
                        fee_ids: value.fee_ids,
                        concerns: value.concerns.trim(),
                        requirement: null,
                    }
                if (
                    ["close", "supplement", "reassign", "renew"].includes(
                        action,
                    )
                )
                    data.reason = value.reason.trim()
                if (
                    ["supplement", "publish", "reassign", "renew"].includes(
                        action,
                    )
                )
                    Object.assign(data, taskBody)
                if (action === "publish") {
                    if (!beforePublish)
                        throw new Error("采购反馈尚未准备好，请稍候再发布。")
                    onPublishingChange?.(true)
                    const context = await beforePublish()
                    if (!context) {
                        setAction(null)
                        return
                    }
                    feedbackSaved = true
                    Object.assign(data, context)
                }
                if (action === "reassign") {
                    if (!value.buyer_user_id)
                        throw new Error("请选择新的责任采购。")
                    data.owner_user_id = value.buyer_user_id
                }
                await command.run(
                    `${inquiryPath(procurement, inquiry.id)}/${action}`,
                    data,
                )
                const completedAction = action
                setAction(null)
                form.reset({
                    ...value,
                    reason: "",
                    concerns: "",
                    line_ids: [],
                    fee_ids: [],
                })
                localStorage.removeItem(storageKey)
                toast.add({
                    title: `${ACTION_LABEL[completedAction]}已完成`,
                    type: "success",
                })
                onSaved()
            } catch (failure) {
                const reason = getErrorMessage(
                    failure,
                    "操作失败，请保留输入并重试。",
                )
                const outcome =
                    classifyFormalCommandError(failure) === "unknown"
                        ? "发布结果尚待核对，请核对上次提交结果后继续。"
                        : "本轮尚未发布。"
                setError(
                    feedbackSaved
                        ? `你的反馈已保存，${outcome}${reason}`
                        : reason,
                )
                if (action === "close") throw failure
            } finally {
                submitting.current = false
                onPublishingChange?.(false)
            }
        },
    })
    const closeReason = useStore(form.store, (state) => state.values.reason)
    const isSubmitting = useStore(form.store, (state) => state.isSubmitting)
    const demandLines = inquiry.draft.lines
    React.useEffect(() => {
        const saved = localStorage.getItem(storageKey)
        if (saved) {
            try {
                form.reset(JSON.parse(saved))
            } catch {
                /* 损坏记录保留初值。 */
            }
        }
        // 仅切换询价时恢复；后台刷新不清除填写中的原因和重询内容。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [storageKey])
    const actions: InquiryAction[] = procurement
        ? task?.status === "OPEN" &&
          ["awaiting_procurement", "procurement_processing"].includes(
              inquiry.status,
          )
            ? ["publish", "supplement", "reassign"]
            : task?.status === "COMPLETED" &&
                ["awaiting_sales", "confirmed"].includes(inquiry.status)
              ? ["renew"]
              : []
        : inquiry.status === "draft" ||
            inquiry.status === "awaiting_sales_supplement"
          ? ["submit", "close"]
          : inquiry.status === "awaiting_sales"
            ? ["reinquire", "accept", "close"]
            : inquiry.status === "confirmed"
              ? ["reinquire", "close"]
              : inquiry.status !== "closed"
                ? ["close"]
                : []
    const permissionForAction = (item: InquiryAction) => {
        const permission = `${procurement ? "procurement_inquiry" : "sales_inquiry"}:${item === "renew" ? "publish" : item}`
        return item === "reassign"
            ? [permission, "procurement_person:list"]
            : [permission]
    }
    const canRunAction = (item: InquiryAction) =>
        permissionForAction(item).every((code) =>
            hasPermission(permissions, code),
        )
    const isPrimaryWorkflowAction = (item: InquiryAction) =>
        item === "submit" ||
        item === "accept" ||
        item === "publish" ||
        item === "renew"

    const renderButton = (item: InquiryAction) => {
        const isPrimary = isPrimaryWorkflowAction(item)
        const submitDirectly = item === "submit" && !submitRequiresReason
        const quiet = item === "close" || item === "reinquire"
        const actionDisabled =
            disabled ||
            (feedbackDirty && item !== "publish") ||
            (item === "publish" && !beforePublish) ||
            isSubmitting ||
            ((item === "accept" || item === "reinquire") &&
                feedbackRound != null &&
                feedbackRound !== inquiry.feedback_round) ||
            command.isPending ||
            command.unresolved ||
            !command.ready ||
            (procurement && !task)

        // 当需求存在未保存修改时，底部 SaveDock 的「保存需求」为实心主蓝，
        // 顶栏「提交采购」退让为 outline，杜绝“保存降级而蓝色主按钮灰掉不可点”的主次倒置。
        const demoteSubmitWhileDirty = item === "submit" && disabled
        const variant = quiet
            ? "ghost"
            : isPrimary && !demoteSubmitWhileDirty
              ? "default"
              : "outline"

        return (
            <PermissionGuard key={item} permission={permissionForAction(item)}>
                <LoadingButton
                    key={item}
                    id={`sales-inquiry-action-${item}`}
                    type="button"
                    size="sm"
                    variant={variant}
                    className={
                        quiet
                            ? item === "close"
                                ? detailWorkspaceStyles.quietDangerButton
                                : detailWorkspaceStyles.quietButton
                            : isPrimary && !demoteSubmitWhileDirty
                              ? "px-3.5"
                              : item === "supplement" || item === "reassign"
                                ? "cursor-pointer"
                                : undefined
                    }
                    disabled={actionDisabled}
                    loading={submitDirectly && action === null && isSubmitting}
                    title={
                        disabled || (feedbackDirty && item !== "publish")
                            ? (disabledReason ?? "请先保存当前修改后再操作")
                            : (item === "accept" || item === "reinquire") &&
                                feedbackRound != null &&
                                feedbackRound !== inquiry.feedback_round
                              ? "请切回最新一轮反馈后再操作"
                              : undefined
                    }
                    onClick={() => {
                        setError(null)
                        if (
                            item === "publish" &&
                            validateBeforePublish &&
                            !validateBeforePublish()
                        ) {
                            return
                        }
                        if (submitDirectly) {
                            void form.handleSubmit(item)
                            return
                        }
                        setAction(item)
                    }}
                >
                    {item === "supplement" && (
                        <FilePenLineIcon
                            aria-hidden="true"
                            data-icon="inline-start"
                        />
                    )}
                    {item === "reassign" && (
                        <UserRoundCogIcon
                            aria-hidden="true"
                            data-icon="inline-start"
                        />
                    )}
                    {ACTION_LABEL[item]}
                </LoadingButton>
            </PermissionGuard>
        )
    }

    const primaryItems = actions.filter(
        (item) => isPrimaryWorkflowAction(item) && canRunAction(item),
    )
    const secondaryItems = actions.filter(
        (item) =>
            (item === "reinquire" || item === "close") && canRunAction(item),
    )
    const assignmentItems = actions.filter(
        (item) =>
            (item === "reassign" || item === "supplement") &&
            canRunAction(item),
    )

    const alerts = (
        <>
            {command.unresolved && (
                <Alert variant="warning" className="shrink-0">
                    <AlertDescription>
                        上次提交结果尚待核对，当前操作暂停。
                        <Button
                            id="sales-inquiry-action-recover"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() =>
                                void command
                                    .recover()
                                    .then(() => onSaved())
                                    .catch((failure) =>
                                        setError(getErrorMessage(failure)),
                                    )
                            }
                        >
                            核对上次提交结果
                        </Button>
                    </AlertDescription>
                </Alert>
            )}
            {error && !action && (
                <Alert variant="destructive" className="shrink-0">
                    <AlertDescription>{error}</AlertDescription>
                </Alert>
            )}
        </>
    )

    return (
        <div
            className={
                fillHeight
                    ? "flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
                    : workspace
                      ? "min-w-0"
                      : renderActions
                        ? "flex min-h-0 min-w-0 flex-1 flex-col gap-3 overflow-hidden"
                        : "space-y-3"
            }
        >
            {!renderActions && alerts}
            {renderActions ? (
                renderActions({
                    primaryAction:
                        primaryItems.length > 0 ? (
                            <>{primaryItems.map(renderButton)}</>
                        ) : undefined,
                    secondaryActions:
                        secondaryItems.length > 0 ? (
                            <>{secondaryItems.map(renderButton)}</>
                        ) : undefined,
                    assignmentActions:
                        assignmentItems.length > 0 ? (
                            <>{assignmentItems.map(renderButton)}</>
                        ) : undefined,
                    alerts,
                })
            ) : (
                <div className="flex flex-wrap gap-2">
                    {actions.map(renderButton)}
                </div>
            )}
            <FormalActionConfirmDialog
                idPrefix="sales-inquiry-close-confirm"
                open={action === "close" && canRunAction(action)}
                onOpenChange={(open) => {
                    if (!open) setAction(null)
                }}
                actionLabel="关闭询价"
                title="关闭询价"
                confirmLabel="确认关闭"
                actionVariant="destructive"
                layout="compact"
                fromStatus={{
                    label: INQUIRY_STATUS_LABEL[inquiry.status],
                    tone: INQUIRY_STATUS_TONE[inquiry.status],
                }}
                toStatus={{ label: "已关闭", tone: "neutral" }}
                summary={[inquiry.inquiry_no, inquiry.customer_name]}
                effects={[
                    "未完成采购任务同步关闭",
                    "已有销售及采购按各自流程继续办理",
                ]}
                irreversibleEffects={[
                    "本询价不能继续处理或新增转单；再次采购须复制为新询价",
                ]}
                pending={command.isPending || isSubmitting}
                confirmDisabled={
                    disabled ||
                    !closeReason.trim() ||
                    command.unresolved ||
                    !command.ready
                }
                onConfirm={() => form.handleSubmit("close")}
                formContent={
                    <div className="space-y-3">
                        {error && (
                            <Alert variant="destructive">
                                <AlertDescription>{error}</AlertDescription>
                            </Alert>
                        )}
                        <form.AppField
                            name="reason"
                            children={(field) => (
                                <field.TextareaField
                                    id="sales-inquiry-close-reason"
                                    label="关闭原因"
                                    required
                                />
                            )}
                        />
                    </div>
                }
            />
            <Dialog
                open={
                    action != null && action !== "close" && canRunAction(action)
                }
                onOpenChange={(open) => {
                    if (!open && !submitting.current) setAction(null)
                }}
            >
                <DialogContent
                    size="lg"
                    scroll="body"
                    closeButtonId="sales-inquiry-action-dialog-close"
                    showCloseButton={!isSubmitting && !command.isPending}
                >
                    <DialogHeader>
                        <DialogTitle>
                            {action ? ACTION_LABEL[action] : "处理询价"}
                        </DialogTitle>
                        <DialogDescription>
                            {action === "accept"
                                ? "接受当前完整反馈。商品供给价仅作成本参考，实际成交价在销售开单时明确确认。"
                                : action === "reinquire"
                                  ? "沿用当前商品、数量、地址及交期，仅要求采购重新报价。需要变更需求时，请返回商品与报价，点击“修改需求”。"
                                  : action === "renew"
                                    ? "将重新创建整单报价任务，保留原报价和已开单依据；新结果发布后须由销售再次接受。"
                                    : action === "submit"
                                      ? "提交后按采购责任规则自动分派，无需选择采购人员。"
                                      : action === "publish"
                                        ? "确认后自动保存当前商品反馈与费用，再统一发布给销售，无需先点保存。内部供应商报价不会公开；其他责任采购尚未完成时，你的内容会保留为已保存进度。"
                                        : action === "close"
                                          ? "关闭后不能新增询价或转单，已生成销售与采购继续按各自流程办理。"
                                          : "核对当前需求和责任范围后执行。"}
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="flex min-h-0 flex-1 flex-col overflow-hidden"
                        onSubmit={(event) => {
                            event.preventDefault()
                            if (action) void form.handleSubmit(action)
                        }}
                    >
                        <DialogBody className="space-y-4">
                            {error && (
                                <Alert variant="destructive">
                                    <AlertDescription>{error}</AlertDescription>
                                </Alert>
                            )}
                            {action && requiresReason(action) && (
                                <form.AppField
                                    name="reason"
                                    children={(field) => (
                                        <field.TextareaField
                                            id="sales-inquiry-action-reason"
                                            label={
                                                action === "submit"
                                                    ? "需求变更原因"
                                                    : "操作原因"
                                            }
                                            required
                                        />
                                    )}
                                />
                            )}
                            {action === "reassign" && (
                                <form.Field
                                    name="buyer_user_id"
                                    children={(field) => (
                                        <PersonDirectoryFilter
                                            id="sales-inquiry-action-buyer"
                                            category="procurement"
                                            value={field.state.value}
                                            onChange={field.handleChange}
                                            selectionMode="single"
                                            label="新的责任采购"
                                        />
                                    )}
                                />
                            )}
                            {action === "reinquire" && (
                                <>
                                    <form.Field
                                        name="line_ids"
                                        children={(field) => (
                                            <div className="space-y-2">
                                                <Label htmlFor="sales-inquiry-reinquire-lines">
                                                    关注商品（不选表示全部）
                                                </Label>
                                                <MultiOptionCombobox
                                                    id="sales-inquiry-reinquire-lines"
                                                    aria-label="关注商品"
                                                    value={field.state.value}
                                                    onValueChange={(values) =>
                                                        field.handleChange([
                                                            ...values,
                                                        ])
                                                    }
                                                    options={demandLines.map(
                                                        (line) => ({
                                                            value: line.line_id,
                                                            label: line.product_name,
                                                        }),
                                                    )}
                                                />
                                            </div>
                                        )}
                                    />
                                    <form.Field
                                        name="fee_ids"
                                        children={(field) => (
                                            <div className="space-y-2">
                                                <Label htmlFor="sales-inquiry-reinquire-fees">
                                                    关注费用（不选表示全部）
                                                </Label>
                                                <MultiOptionCombobox
                                                    id="sales-inquiry-reinquire-fees"
                                                    aria-label="关注费用"
                                                    value={field.state.value}
                                                    onValueChange={(values) =>
                                                        field.handleChange([
                                                            ...values,
                                                        ])
                                                    }
                                                    options={(
                                                        inquiry.feedbacks.at(-1)
                                                            ?.data.fees ?? []
                                                    ).map((fee) => ({
                                                        value: fee.fee_id,
                                                        label: fee.name,
                                                    }))}
                                                />
                                            </div>
                                        )}
                                    />
                                    <form.AppField
                                        name="concerns"
                                        children={(field) => (
                                            <field.TextareaField
                                                id="sales-inquiry-reinquire-concerns"
                                                label="需要调整的事项"
                                                placeholder="价格、交期、供给数量或费用等"
                                            />
                                        )}
                                    />
                                </>
                            )}
                        </DialogBody>
                        <DialogFooter>
                            <Button
                                id="sales-inquiry-action-cancel"
                                type="button"
                                variant="outline"
                                onClick={() => setAction(null)}
                                disabled={isSubmitting || command.isPending}
                            >
                                取消
                            </Button>
                            <Button
                                id="sales-inquiry-action-confirm"
                                type="submit"
                                disabled={
                                    disabled ||
                                    isSubmitting ||
                                    command.isPending ||
                                    command.unresolved ||
                                    !command.ready
                                }
                            >
                                {isSubmitting || command.isPending
                                    ? action === "publish"
                                        ? "正在保存并发布…"
                                        : "正在提交…"
                                    : action
                                      ? ACTION_LABEL[action]
                                      : "确认"}
                            </Button>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
        </div>
    )
}
