"use client"

import * as React from "react"
import { CopyIcon, FilePlus2Icon, GitBranchIcon } from "lucide-react"
import { useAccountProfileQuery } from "@/features/auth/hooks/queries"
import { hasPermission } from "@/lib/permissions"
import { PROCESS_PERMISSIONS } from "../permissions"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { copyDraftNodes } from "../draft-nodes"
import { definitionErrorMessage, newCommandKey } from "../errors"
import { documentTypeLabel, versionLabel } from "../labels"
import {
    useCreateDefinitionDraftMutation,
    useReplaceDefinitionNodesMutation,
} from "../queries"
import { createDraftSchema } from "../schema"
import type {
    CreateDefinitionDraftRequest,
    DefinitionCatalogItem,
    DefinitionDetailView,
    ReplaceDefinitionNodesCommand,
} from "../types"
import {
    buildCreateDraftRequest,
    buildStableReplaceNodesCommand,
    lockVersionText,
} from "../write-payload"
import { CopyFlowDialog } from "./copy-flow-dialog"

type CreationAttempt = {
    signature: string
    request: CreateDefinitionDraftRequest
    source: DefinitionDetailView | null
    created?: DefinitionDetailView
    copyCommand?: ReplaceDefinitionNodesCommand
}

/** 创建可编辑草稿；跨流程复制沿目标类型的节点保存接口重新校验。 */
export function CreateDraftDialog({
    item,
    open,
    onOpenChange,
    onCreated,
    initialSource = "EMPTY",
    id = "governance-approval-processes-create-draft-dialog",
}: {
    item: DefinitionCatalogItem | null
    open: boolean
    onOpenChange: (open: boolean) => void
    onCreated: (
        definitionId: string,
        documentType: DefinitionCatalogItem["document_type"],
    ) => void
    initialSource?: "EMPTY" | "CURRENT_PUBLISHED"
    id?: string
}) {
    const profile = useAccountProfileQuery()
    const canCopyOther = hasPermission(
        profile.data?.permissions,
        PROCESS_PERMISSIONS.edit,
    )
    const createDraft = useCreateDefinitionDraftMutation()
    const replaceNodes = useReplaceDefinitionNodesMutation()
    const attempt = React.useRef<CreationAttempt | null>(null)
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const [copyOpen, setCopyOpen] = React.useState(false)
    const [createdDraft, setCreatedDraft] =
        React.useState<DefinitionDetailView | null>(null)
    const pending = createDraft.isPending || replaceNodes.isPending
    const canCopy = Boolean(item?.published_version)
    const finish = (detail: DefinitionDetailView) => {
        onOpenChange(false)
        onCreated(detail.definition_id, detail.document_type)
    }
    const form = useAppForm({
        defaultValues: {
            name: item ? `${item.document_type_label}审批` : "",
            draft_source: initialSource as
                | "EMPTY"
                | "CURRENT_PUBLISHED"
                | "OTHER",
            copy_source: null as DefinitionDetailView | null,
        },
        validators: { onSubmit: createDraftSchema },
        onSubmit: async ({ value }) => {
            if (!item) return
            if (value.draft_source === "OTHER" && !canCopyOther) {
                setSubmitError("当前账号没有编辑审批流程的权限。")
                return
            }
            const source =
                value.draft_source === "OTHER" ? value.copy_source : null
            if (value.draft_source === "OTHER" && !source?.nodes.length) {
                setSubmitError("请选择包含审批节点的来源流程。")
                return
            }
            if (value.draft_source === "CURRENT_PUBLISHED" && !canCopy) {
                setSubmitError("当前没有可复制的已发布版本。")
                return
            }
            setSubmitError(null)
            const signature = JSON.stringify([
                item.document_type,
                value.name.trim(),
                value.draft_source,
                source,
            ])
            if (
                !attempt.current ||
                (!attempt.current.created &&
                    attempt.current.signature !== signature)
            ) {
                attempt.current = {
                    signature,
                    source,
                    request: buildCreateDraftRequest(
                        item.document_type,
                        value.name,
                        value.draft_source === "CURRENT_PUBLISHED"
                            ? "CURRENT_PUBLISHED"
                            : "EMPTY",
                        newCommandKey("create-draft"),
                    ),
                }
            }
            const current = attempt.current
            try {
                if (!current.created)
                    current.created = await createDraft.mutateAsync(
                        current.request,
                    )
                setCreatedDraft(current.created)
                let detail = current.created
                if (current.source) {
                    current.copyCommand ??= buildStableReplaceNodesCommand(
                        detail.definition_id,
                        lockVersionText(detail.definition_lock_version),
                        copyDraftNodes(current.source.nodes),
                        null,
                        () => newCommandKey("copy-flow"),
                        current.source.copy_rules ?? [],
                    )
                    detail = await replaceNodes.mutateAsync(current.copyCommand)
                }
                finish(detail)
            } catch (error) {
                setSubmitError(definitionErrorMessage(error))
            }
        },
    })
    React.useEffect(() => {
        if (!open) return
        form.reset({
            name: item ? `${item.document_type_label}审批` : "",
            draft_source: initialSource,
            copy_source: null,
        })
        attempt.current = null
        setCreatedDraft(null)
        setSubmitError(null)
        setCopyOpen(false)
        // 仅在打开对话框或切换目标类型时初始化。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, item?.document_type])
    if (!item) return null
    return (
        <>
            <Dialog
                open={open}
                onOpenChange={(next) => {
                    if (!pending) onOpenChange(next)
                }}
            >
                <DialogContent
                    closeButtonId={`${id}-close`}
                    className="max-h-[90dvh] overflow-y-auto sm:max-w-lg"
                >
                    <DialogHeader>
                        <DialogTitle>新建审批草稿</DialogTitle>
                        <DialogDescription>
                            {documentTypeLabel(
                                item.document_type,
                                item.document_type_label,
                            )}{" "}
                            · 保存为草稿，发布后用于新建单据。
                        </DialogDescription>
                    </DialogHeader>
                    {submitError && (
                        <Alert variant="destructive">
                            <AlertTitle>
                                {createdDraft
                                    ? "草稿已创建，节点尚未复制完成"
                                    : "未能创建草稿"}
                            </AlertTitle>
                            <AlertDescription>
                                {submitError}
                                {createdDraft
                                    ? " 可重试复制，或打开草稿重新配置审批人。"
                                    : ""}
                            </AlertDescription>
                        </Alert>
                    )}
                    <form
                        className="space-y-5"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <form.AppField name="name">
                            {(field) => (
                                <field.TextField
                                    id={`${id}-name`}
                                    label="审批流程名称"
                                    required
                                    disabled={pending || Boolean(createdDraft)}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="draft_source">
                            {(field) => (
                                <div className="space-y-2">
                                    <Label id={`${id}-draft-source-label`}>
                                        创建方式
                                    </Label>
                                    <RadioGroup
                                        id={`${id}-draft-source`}
                                        aria-labelledby={`${id}-draft-source-label`}
                                        value={field.state.value}
                                        className="gap-2"
                                        onValueChange={(value) => {
                                            if (
                                                value === "EMPTY" ||
                                                value === "CURRENT_PUBLISHED" ||
                                                value === "OTHER"
                                            )
                                                field.handleChange(value)
                                        }}
                                    >
                                        {[
                                            {
                                                value: "EMPTY",
                                                label: "空白流程",
                                                description:
                                                    "从第一个审批节点开始配置",
                                                icon: FilePlus2Icon,
                                                disabled: false,
                                                suffix: "empty",
                                            },
                                            {
                                                value: "CURRENT_PUBLISHED",
                                                label: "复制当前版本",
                                                description: canCopy
                                                    ? "保留当前流程的节点和审批人"
                                                    : "当前没有已发布版本",
                                                icon: GitBranchIcon,
                                                disabled: !canCopy,
                                                suffix: "current-published",
                                            },
                                            {
                                                value: "OTHER",
                                                label: "从其它审批流复制",
                                                description: canCopyOther
                                                    ? "复用其它单据类型的流程配置"
                                                    : "需要审批流程编辑权限",
                                                icon: CopyIcon,
                                                disabled: !canCopyOther,
                                                suffix: "other",
                                            },
                                        ].map((option) => (
                                            <Label
                                                key={option.value}
                                                htmlFor={`${id}-draft-source-${option.suffix}`}
                                                className="flex items-center gap-3 rounded-lg border border-border p-3 font-normal has-[[data-checked]]:border-primary/50 has-[[data-checked]]:bg-accent/40"
                                            >
                                                <RadioGroupItem
                                                    id={`${id}-draft-source-${option.suffix}`}
                                                    value={option.value}
                                                    disabled={
                                                        option.disabled ||
                                                        pending ||
                                                        Boolean(createdDraft)
                                                    }
                                                    nativeButton
                                                    render={
                                                        <button
                                                            type="button"
                                                            aria-label={
                                                                option.label
                                                            }
                                                        />
                                                    }
                                                />
                                                <option.icon className="size-4 shrink-0 text-muted-foreground" />
                                                <span className="space-y-1">
                                                    <span className="block text-sm font-medium">
                                                        {option.label}
                                                    </span>
                                                    <span className="block text-xs text-muted-foreground">
                                                        {option.description}
                                                    </span>
                                                </span>
                                            </Label>
                                        ))}
                                    </RadioGroup>
                                </div>
                            )}
                        </form.AppField>
                        <form.Subscribe
                            selector={(state) =>
                                [
                                    state.values.draft_source,
                                    state.values.copy_source,
                                ] as const
                            }
                        >
                            {([source, copied]) =>
                                source === "OTHER" ? (
                                    <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-muted/40 p-3">
                                        <div className="min-w-0">
                                            <p className="text-sm font-medium">
                                                {copied
                                                    ? documentTypeLabel(
                                                          copied.document_type,
                                                          copied.document_type_label,
                                                      )
                                                    : "尚未选择来源"}
                                            </p>
                                            {copied && (
                                                <p className="mt-1 text-xs text-muted-foreground">
                                                    {versionLabel(
                                                        copied.definition_version,
                                                    )}{" "}
                                                    · {copied.nodes.length}{" "}
                                                    个节点
                                                </p>
                                            )}
                                        </div>
                                        <Button
                                            id={`${id}-choose-source`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={
                                                pending || Boolean(createdDraft)
                                            }
                                            onClick={() => setCopyOpen(true)}
                                        >
                                            {copied ? "更换来源" : "选择流程"}
                                        </Button>
                                    </div>
                                ) : null
                            }
                        </form.Subscribe>
                        <DialogFooter>
                            {createdDraft ? (
                                <Button
                                    id={`${id}-open-created`}
                                    type="button"
                                    variant="outline"
                                    disabled={pending}
                                    onClick={() => finish(createdDraft)}
                                >
                                    打开草稿调整
                                </Button>
                            ) : (
                                <Button
                                    id={`${id}-cancel`}
                                    type="button"
                                    variant="outline"
                                    disabled={pending}
                                    onClick={() => onOpenChange(false)}
                                >
                                    取消
                                </Button>
                            )}
                            <form.AppForm>
                                <form.SubmitButton
                                    loading={pending}
                                    id={`${id}-submit`}
                                    label={
                                        createdDraft ? "重试复制" : "创建草稿"
                                    }
                                    disabled={pending}
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                    <CopyFlowDialog
                        id={`${id}-copy`}
                        targetType={item.document_type}
                        open={copyOpen && open}
                        onOpenChange={setCopyOpen}
                        onCopy={(source) =>
                            form.setFieldValue("copy_source", source)
                        }
                    />
                </DialogContent>
            </Dialog>
        </>
    )
}
