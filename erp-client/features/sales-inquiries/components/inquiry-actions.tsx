"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { FormalActionConfirmDialog } from "@/components/business"
import { useAppForm } from "@/components/form"
import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"
import { inquiryPath } from "../api"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { inquiryStorageKey } from "../lib/local-draft"
import type {
    InquiryRequirement,
    ProcurementInquiry,
    SalesInquiry,
} from "../types"
import type { InquiryTask } from "./inquiry-feedback-editor"
import { INQUIRY_STATUS_LABEL } from "../types"
import { InquiryDemandEditor } from "./inquiry-demand-editor"

type InquiryAction =
    | "submit"
    | "accept"
    | "reinquire"
    | "close"
    | "claim"
    | "supplement"
    | "publish"
    | "reassign"
const ACTION_LABEL: Record<InquiryAction, string> = {
    submit: "提交采购",
    accept: "接受本轮结果",
    reinquire: "要求重新询价",
    close: "关闭询价",
    claim: "接手处理",
    supplement: "要求补充需求",
    publish: "发布本轮反馈",
    reassign: "改派责任采购",
}

export function InquiryActions({
    inquiry,
    procurement,
    task,
    userId,
    permissions,
    onSaved,
    disabled = false,
    renderActions,
    feedbackRound,
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    procurement: boolean
    task?: InquiryTask
    userId: string
    permissions: readonly string[]
    onSaved: () => void
    disabled?: boolean
    feedbackRound?: number
    renderActions?: (
        primaryActions: React.ReactNode,
        secondaryActions: React.ReactNode,
    ) => React.ReactNode
}) {
    const [action, setAction] = React.useState<InquiryAction | null>(null)
    const [error, setError] = React.useState<string | null>(null)
    const [editingRequirement, setEditingRequirement] = React.useState(false)
    const requiresReason =
        action === "submit"
            ? inquiry.requirement_version > 0
            : action != null &&
              ["reinquire", "close", "supplement", "reassign"].includes(action)
    const command = useInquiryCommand(userId, inquiry.id)
    const storageKey = `erp.sales-inquiry-action:${encodeURIComponent(userId)}:${encodeURIComponent(inquiry.id)}`
    const form = useAppForm({
        defaultValues: {
            reason: "",
            concerns: "",
            line_ids: [] as string[],
            fee_ids: [] as string[],
            buyer_user_id: "",
            requirement: null as InquiryRequirement | null,
        },
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
        onSubmit: async ({ value }) => {
            if (!action || disabled) return
            setError(null)
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
                if (requiresReason && !value.reason.trim())
                    throw new Error("请填写本次操作原因。")
                if (action === "submit")
                    data.reason = requiresReason ? value.reason.trim() : ""
                if (action === "accept")
                    data.feedback_round = inquiry.feedback_round
                if (action === "reinquire")
                    data.data = {
                        reason: value.reason.trim(),
                        line_ids: value.line_ids,
                        fee_ids: value.fee_ids,
                        concerns: value.concerns.trim(),
                        requirement: value.requirement,
                    }
                if (["close", "supplement", "reassign"].includes(action))
                    data.reason = value.reason.trim()
                if (
                    ["claim", "supplement", "publish", "reassign"].includes(
                        action,
                    )
                )
                    Object.assign(data, taskBody)
                if (action === "reassign") {
                    if (!value.buyer_user_id)
                        throw new Error("请选择新的责任采购。")
                    data.owner_user_id = value.buyer_user_id
                }
                await command.run(
                    `${inquiryPath(procurement, inquiry.id)}/${action}`,
                    data,
                )
                setAction(null)
                setEditingRequirement(false)
                form.reset({
                    ...value,
                    reason: "",
                    concerns: "",
                    line_ids: [],
                    fee_ids: [],
                    requirement: null,
                })
                localStorage.removeItem(storageKey)
                localStorage.removeItem(
                    inquiryStorageKey(userId, `${inquiry.id}:reinquiry-demand`),
                )
                onSaved()
            } catch (failure) {
                setError(
                    getErrorMessage(failure, "操作失败，请保留输入并重试。"),
                )
                if (action === "close") throw failure
            }
        },
    })
    const closeReason = useStore(form.store, (state) => state.values.reason)
    const revisedRequirement = useStore(
        form.store,
        (state) => state.values.requirement,
    )
    const demandLines = (revisedRequirement ?? inquiry.draft).lines
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
        ? inquiry.status === "awaiting_procurement"
            ? ["claim", "reassign"]
            : inquiry.status === "procurement_processing"
              ? ["publish", "supplement", "reassign"]
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
    const renderButton = (item: InquiryAction) => (
        <Button
            key={item}
            id={`sales-inquiry-action-${item}`}
            type="button"
            variant={
                ["close", "reassign", "supplement", "reinquire"].includes(item)
                    ? "outline"
                    : "default"
            }
            disabled={
                disabled ||
                ((item === "accept" || item === "reinquire") &&
                    feedbackRound != null &&
                    feedbackRound !== inquiry.feedback_round) ||
                command.isPending ||
                command.unresolved ||
                !command.ready ||
                (procurement && !task) ||
                !hasPermission(
                    permissions,
                    `${procurement ? "procurement_inquiry" : "sales_inquiry"}:${item}`,
                )
            }
            title={
                disabled
                    ? "请先保存当前修改后再操作"
                    : (item === "accept" || item === "reinquire") &&
                        feedbackRound != null &&
                        feedbackRound !== inquiry.feedback_round
                      ? "请切回最新一轮反馈后再操作"
                      : undefined
            }
            onClick={() => {
                setAction(item)
                setError(null)
            }}
        >
            {ACTION_LABEL[item]}
        </Button>
    )
    return (
        <div className="space-y-3">
            {command.unresolved && (
                <Alert variant="warning">
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
                <Alert variant="destructive">
                    <AlertDescription>{error}</AlertDescription>
                </Alert>
            )}
            {renderActions ? (
                renderActions(
                    actions
                        .filter((item) => item !== "close")
                        .map(renderButton),
                    actions
                        .filter((item) => item === "close")
                        .map(renderButton),
                )
            ) : (
                <div className="flex flex-wrap gap-2">
                    {actions.map(renderButton)}
                </div>
            )}
            <FormalActionConfirmDialog
                idPrefix="sales-inquiry-close-confirm"
                open={action === "close"}
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
                    tone: "neutral",
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
                pending={command.isPending}
                confirmDisabled={
                    disabled ||
                    !closeReason.trim() ||
                    command.unresolved ||
                    !command.ready
                }
                onConfirm={() => form.handleSubmit()}
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
                open={action != null && action !== "close"}
                onOpenChange={(open) => {
                    if (!open) setAction(null)
                }}
            >
                <DialogContent
                    closeButtonId="sales-inquiry-action-dialog-close"
                    className="max-h-[85svh] overflow-y-auto sm:max-w-2xl"
                >
                    <DialogHeader>
                        <DialogTitle>
                            {action ? ACTION_LABEL[action] : "处理询价"}
                        </DialogTitle>
                        <DialogDescription>
                            {action === "accept"
                                ? "接受当前完整反馈。商品供给价仅作成本参考，实际成交价在销售开单时明确确认。"
                                : action === "submit" || action === "reinquire"
                                  ? "提交后按采购责任规则自动分派，无需选择采购人员。"
                                  : action === "publish"
                                    ? "发布前请先保存处理进度；所有责任商品必须有最终供给结论。已发布内容冻结为本轮完整结果。"
                                    : action === "close"
                                      ? "关闭后不能新增询价或转单，已生成销售与采购继续按各自流程办理。"
                                      : "核对当前需求和责任范围后执行。"}
                        </DialogDescription>
                    </DialogHeader>
                    {editingRequirement && "sales_owner_user_id" in inquiry ? (
                        <div className="space-y-4">
                            <Button
                                id="sales-inquiry-revision-back"
                                type="button"
                                variant="outline"
                                onClick={() => setEditingRequirement(false)}
                            >
                                返回重询说明
                            </Button>
                            <InquiryDemandEditor
                                inquiry={{
                                    ...inquiry,
                                    draft: revisedRequirement ?? inquiry.draft,
                                }}
                                userId={userId}
                                permissions={permissions}
                                storageScope={`${inquiry.id}:reinquiry-demand`}
                                onSaved={() => undefined}
                                onConfirmRequirement={(requirement) => {
                                    form.setFieldValue(
                                        "requirement",
                                        requirement,
                                    )
                                    setEditingRequirement(false)
                                }}
                            />
                        </div>
                    ) : (
                        <form
                            className="space-y-4"
                            onSubmit={(event) => {
                                event.preventDefault()
                                void form.handleSubmit()
                            }}
                        >
                            {error && (
                                <Alert variant="destructive">
                                    <AlertDescription>{error}</AlertDescription>
                                </Alert>
                            )}
                            {requiresReason && (
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
                                    <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3">
                                        <p className="text-sm">
                                            {revisedRequirement
                                                ? "本次将提交调整后的需求，并建立新的需求版本。"
                                                : "商品、数量、地址或交期变化时，先调整需求。"}
                                        </p>
                                        <Button
                                            id="sales-inquiry-reinquire-edit-demand"
                                            type="button"
                                            variant="outline"
                                            onClick={() =>
                                                setEditingRequirement(true)
                                            }
                                        >
                                            调整商品、数量及地址
                                        </Button>
                                    </div>
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
                            <DialogFooter>
                                <Button
                                    id="sales-inquiry-action-cancel"
                                    type="button"
                                    variant="outline"
                                    onClick={() => setAction(null)}
                                >
                                    取消
                                </Button>
                                <Button
                                    id="sales-inquiry-action-confirm"
                                    type="submit"
                                    disabled={
                                        command.isPending || command.unresolved
                                    }
                                >
                                    {command.isPending
                                        ? "正在提交…"
                                        : action
                                          ? ACTION_LABEL[action]
                                          : "确认"}
                                </Button>
                            </DialogFooter>
                        </form>
                    )}
                </DialogContent>
            </Dialog>
        </div>
    )
}
