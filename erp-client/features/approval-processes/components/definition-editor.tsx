"use client"

import * as React from "react"
import { BellIcon, PlusIcon } from "lucide-react"
import { DraftSaveIndicator } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import { CopyRuleEditor } from "./copy-rule-editor"
import { seedCopyRules, buildCopyRules } from "../copy-rules"
import {
    copyDraftNodes,
    emptyEditorNode,
    orderNodesForSave,
    seedDraftNodes,
} from "../draft-nodes"
import { definitionErrorMessage, newCommandKey } from "../errors"
import { rejectionPolicyCopy } from "../labels"
import { useReplaceDefinitionNodesMutation } from "../queries"
import { definitionEditorSchema } from "../schema"
import type {
    DefinitionDetailView,
    ReplaceDefinitionNodesCommand,
} from "../types"
import {
    buildStableReplaceNodesCommand,
    lockVersionText,
} from "../write-payload"
import { CopyFlowDialog } from "./copy-flow-dialog"
import { DefinitionFlowCanvas } from "./definition-flow-canvas"
import { NodeInlineEditor } from "./node-inline-editor"
import {
    insertionIndex,
    type NodeInsertion,
    type NodeInsertionResult,
} from "./flow-node-insertion"

const INCOMPLETE_DRAFT_MESSAGE = "请补全每个节点的名称和审批人后再保存。"

/** 流程图内展开当前节点配置，所有修改由同一草稿表单保存。 */
export function DefinitionEditor({
    detail,
    lockVersion,
    onLockVersionChange,
    onSaved,
    onDirtyChange,
    publishAction,
    id = "governance-approval-processes-detail-editor",
}: {
    detail: DefinitionDetailView
    lockVersion: string
    onLockVersionChange: (next: string) => void
    onSaved?: (next: DefinitionDetailView) => void
    onDirtyChange?: (dirty: boolean) => void
    publishAction?: React.ReactNode
    id?: string
}) {
    const replaceNodes = useReplaceDefinitionNodesMutation()
    const pendingCommandRef =
        React.useRef<ReplaceDefinitionNodesCommand | null>(null)
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const [saveState, setSaveState] = React.useState<
        "idle" | "saving" | "saved" | "failed"
    >("idle")
    const [savedAt, setSavedAt] = React.useState<Date | undefined>()
    const [selectedNodeId, setSelectedNodeId] = React.useState<string | null>(
        null,
    )
    const [copyOpen, setCopyOpen] = React.useState(false)
    const [copySettings, setCopySettings] = React.useState<{
        nodeClientId: string | null
    } | null>(null)
    const copyReturnFocus = React.useRef<HTMLElement | null>(null)
    const readOnly = detail.status !== "DRAFT"
    const initialNodes = React.useMemo(
        () => seedDraftNodes(detail.document_type, detail.nodes),
        [detail.document_type, detail.nodes],
    )
    const form = useAppForm({
        defaultValues: {
            name: detail.name,
            nodes: initialNodes,
            copyRules: seedCopyRules(detail, initialNodes),
        },
        validators: {
            onChange: definitionEditorSchema,
            onSubmit: definitionEditorSchema,
        },
        onSubmitInvalid: () => {
            setSubmitError(INCOMPLETE_DRAFT_MESSAGE)
            setSaveState("failed")
            const invalid = form.state.values.nodes.find(
                (node) =>
                    !node.node_name.trim() ||
                    (node.assignee_source === "SPECIFIED" &&
                        !node.assignee_user_id),
            )
            if (invalid) setSelectedNodeId(invalid.client_id)
        },
        onSubmit: async ({ value }) => {
            if (readOnly) return
            setSubmitError(null)
            setSaveState("saving")
            try {
                const command = buildStableReplaceNodesCommand(
                    detail.definition_id,
                    lockVersion,
                    orderNodesForSave(detail.document_type, value.nodes),
                    pendingCommandRef.current,
                    () => newCommandKey("replace-nodes"),
                    buildCopyRules(
                        value.copyRules,
                        orderNodesForSave(detail.document_type, value.nodes),
                    ),
                )
                pendingCommandRef.current = command
                const next = await replaceNodes.mutateAsync(command)
                pendingCommandRef.current = null
                const nextNodes = seedDraftNodes(next.document_type, next.nodes)
                const selectedIndex = value.nodes.findIndex(
                    (node) => node.client_id === selectedNodeId,
                )
                form.reset({
                    name: next.name,
                    nodes: nextNodes,
                    copyRules: seedCopyRules(next, nextNodes),
                })
                setSelectedNodeId(
                    nextNodes[Math.max(0, selectedIndex)]?.client_id ?? null,
                )
                onLockVersionChange(
                    lockVersionText(next.definition_lock_version),
                )
                onSaved?.(next)
                setSaveState("saved")
                setSavedAt(new Date())
                toast.add({
                    title: "草稿已保存",
                    type: "success",
                    timeout: 4000,
                })
            } catch (error) {
                setSubmitError(definitionErrorMessage(error))
                setSaveState("failed")
            }
        },
    })
    const insertNode = (
        placement: NodeInsertion,
    ): NodeInsertionResult | null => {
        if (readOnly || replaceNodes.isPending) return null
        const nodes = form.state.values.nodes
        if (nodes.length >= 20) {
            toast.add({ title: "最多配置 20 个审批步骤", type: "warning" })
            return null
        }
        const index = insertionIndex(nodes, placement)
        if (index === null) {
            toast.add({
                title: "目标节点已变更，请重新打开节点菜单",
                type: "warning",
            })
            return null
        }
        const node = emptyEditorNode()
        const next = [...nodes]
        next.splice(index, 0, node)
        form.setFieldValue("nodes", next)
        setSelectedNodeId(node.client_id)
        setSaveState("idle")
        setSubmitError(null)
        return { node, nodes: next }
    }
    const removeNode = (clientId: string): boolean => {
        if (readOnly || replaceNodes.isPending) return false
        const nodes = form.state.values.nodes
        const index = nodes.findIndex((node) => node.client_id === clientId)
        if (index < 0) {
            toast.add({
                title: "目标节点已变更，请重新打开节点菜单",
                type: "warning",
            })
            return false
        }
        const next = nodes.filter((node) => node.client_id !== clientId)
        form.setFieldValue("nodes", next)
        form.setFieldValue(
            "copyRules",
            form.state.values.copyRules.filter(
                (rule) => rule.nodeClientId !== clientId,
            ),
        )
        setSelectedNodeId((selected) =>
            selected &&
            selected !== clientId &&
            next.some((node) => node.client_id === selected)
                ? selected
                : (next[Math.min(index, next.length - 1)]?.client_id ?? null),
        )
        setSaveState("idle")
        setSubmitError(null)
        return true
    }
    const addNode = () => {
        insertNode({ position: "end" })
    }
    const openCopies = (nodeClientId: string | null) => {
        copyReturnFocus.current =
            document.activeElement instanceof HTMLElement
                ? document.activeElement
                : null
        setCopySettings({ nodeClientId })
    }
    return (
        <>
            <form
                id={`${id}-form`}
                className="flex min-w-0 flex-col"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <form.Subscribe selector={(state) => state.isDirty}>
                    {(dirty) => (
                        <DirtyObserver dirty={dirty} onChange={onDirtyChange} />
                    )}
                </form.Subscribe>
                <div className="space-y-4 p-4 sm:p-6">
                    {submitError && (
                        <Alert variant="destructive">
                            <AlertTitle>保存失败</AlertTitle>
                            <AlertDescription>{submitError}</AlertDescription>
                        </Alert>
                    )}
                    <form.Subscribe
                        selector={(state) =>
                            [
                                state.values.nodes,
                                state.values.copyRules,
                            ] as const
                        }
                    >
                        {([nodes, copyRules]) => {
                            const selectedNode =
                                nodes.find(
                                    (node) => node.client_id === selectedNodeId,
                                ) ??
                                nodes[0] ??
                                null
                            const nodeCopyCount = selectedNode
                                ? copyRules
                                      .filter(
                                          (rule) =>
                                              rule.trigger ===
                                                  "NODE_APPROVED" &&
                                              rule.nodeClientId ===
                                                  selectedNode.client_id,
                                      )
                                      .reduce(
                                          (count, rule) =>
                                              count + rule.recipients.length,
                                          0,
                                      )
                                : 0
                            const globalRuleCount = copyRules.filter(
                                (rule) => !rule.nodeClientId,
                            ).length
                            return (
                                <div className="space-y-4">
                                    <DefinitionFlowCanvas
                                        id={`${id}-flow`}
                                        definitionId={detail.definition_id}
                                        documentType={detail.document_type}
                                        onInsertNode={insertNode}
                                        onRemoveNode={removeNode}
                                        disabled={
                                            readOnly || replaceNodes.isPending
                                        }
                                        nodes={nodes}
                                        onCopyFlow={() => setCopyOpen(true)}
                                        selectedClientId={
                                            selectedNode?.client_id ?? null
                                        }
                                        onSelect={setSelectedNodeId}
                                        onReorder={(next) => {
                                            setSelectedNodeId(
                                                selectedNodeId ??
                                                    nodes[0]?.client_id ??
                                                    null,
                                            )
                                            form.setFieldValue("nodes", next)
                                            setSaveState("idle")
                                        }}
                                        sidebar={
                                            <div className="flex h-full flex-col">
                                                <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border px-4 py-2">
                                                    <span className="text-xs font-medium text-muted-foreground">
                                                        流程规则
                                                    </span>
                                                    <Button
                                                        id={`${id}-global-copies`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="xs"
                                                        disabled={
                                                            readOnly ||
                                                            replaceNodes.isPending
                                                        }
                                                        onClick={() =>
                                                            openCopies(null)
                                                        }
                                                    >
                                                        <BellIcon
                                                            aria-hidden
                                                            data-icon="inline-start"
                                                        />
                                                        全局抄送
                                                        <span className="text-muted-foreground">
                                                            {globalRuleCount
                                                                ? `${globalRuleCount} 条`
                                                                : "未设置"}
                                                        </span>
                                                    </Button>
                                                </div>
                                                <div className="flex-1 p-4">
                                                    {selectedNode ? (
                                                        <NodeInlineEditor
                                                            id={`${id}-nodes`}
                                                            nodes={nodes}
                                                            documentType={
                                                                detail.document_type
                                                            }
                                                            readOnly={
                                                                readOnly ||
                                                                replaceNodes.isPending
                                                            }
                                                            node={selectedNode}
                                                            copyCount={
                                                                nodeCopyCount
                                                            }
                                                            onConfigureCopies={() =>
                                                                openCopies(
                                                                    selectedNode.client_id,
                                                                )
                                                            }
                                                            onChange={(
                                                                next,
                                                            ) => {
                                                                const activeId =
                                                                    nodes.find(
                                                                        (
                                                                            node,
                                                                        ) =>
                                                                            node.client_id ===
                                                                            selectedNodeId,
                                                                    )
                                                                        ?.client_id ??
                                                                    nodes[0]
                                                                        ?.client_id
                                                                const oldIndex =
                                                                    nodes.findIndex(
                                                                        (
                                                                            node,
                                                                        ) =>
                                                                            node.client_id ===
                                                                            activeId,
                                                                    )
                                                                setSelectedNodeId(
                                                                    next.find(
                                                                        (
                                                                            node,
                                                                        ) =>
                                                                            node.client_id ===
                                                                            activeId,
                                                                    )
                                                                        ?.client_id ??
                                                                        next[
                                                                            Math.max(
                                                                                0,
                                                                                Math.min(
                                                                                    oldIndex,
                                                                                    next.length -
                                                                                        1,
                                                                                ),
                                                                            )
                                                                        ]
                                                                            ?.client_id ??
                                                                        null,
                                                                )
                                                                form.setFieldValue(
                                                                    "nodes",
                                                                    next,
                                                                )
                                                                form.setFieldValue(
                                                                    "copyRules",
                                                                    form.state.values.copyRules.filter(
                                                                        (r) =>
                                                                            !r.nodeClientId ||
                                                                            next.some(
                                                                                (
                                                                                    n,
                                                                                ) =>
                                                                                    n.client_id ===
                                                                                    r.nodeClientId,
                                                                            ),
                                                                    ),
                                                                )
                                                                setSaveState(
                                                                    "idle",
                                                                )
                                                            }}
                                                        />
                                                    ) : (
                                                        <div className="space-y-3">
                                                            <h3 className="text-sm font-medium">
                                                                还没有审批步骤
                                                            </h3>
                                                            <p className="text-xs leading-5 text-muted-foreground">
                                                                流程会从提交申请直达审批完成。添加步骤后会自动连线。
                                                            </p>
                                                            <Button
                                                                id={`${id}-empty-add`}
                                                                type="button"
                                                                variant="outline"
                                                                onClick={
                                                                    addNode
                                                                }
                                                            >
                                                                <PlusIcon data-icon="inline-start" />
                                                                添加第一个步骤
                                                            </Button>
                                                        </div>
                                                    )}
                                                </div>
                                            </div>
                                        }
                                    />
                                    <p className="px-1 text-xs leading-5 text-muted-foreground">
                                        <span className="font-medium text-foreground">
                                            驳回规则：
                                        </span>
                                        {rejectionPolicyCopy(
                                            detail.document_type,
                                        )}
                                    </p>
                                </div>
                            )
                        }}
                    </form.Subscribe>
                </div>
                <div className="sticky bottom-0 z-10 flex flex-wrap items-center justify-between gap-3 border-t border-border bg-card px-4 py-3 sm:px-6">
                    {saveState === "idle" ? (
                        <form.Subscribe
                            selector={(state) =>
                                [state.isDirty, state.values.nodes] as const
                            }
                        >
                            {([dirty, nodes]) => {
                                const incomplete = nodes.filter(
                                    (node) =>
                                        !node.node_name.trim() ||
                                        (node.assignee_source === "SPECIFIED" &&
                                            !node.assignee_user_id),
                                ).length
                                return (
                                    <span className="text-xs text-muted-foreground">
                                        {nodes.length === 0
                                            ? "请添加至少一个审批步骤"
                                            : incomplete
                                              ? `${incomplete} 个步骤待补全，点击画布节点或右侧面板编辑`
                                              : dirty
                                                ? "有未保存的修改，保存后方可发布"
                                                : "草稿已保存，尚未发布；当前生效流程不受影响"}
                                    </span>
                                )
                            }}
                        </form.Subscribe>
                    ) : (
                        <DraftSaveIndicator
                            state={saveState}
                            savedAt={savedAt}
                            message={
                                saveState === "failed"
                                    ? (submitError ?? undefined)
                                    : undefined
                            }
                        />
                    )}
                    <div className="flex flex-wrap items-center gap-2">
                        <form.AppForm>
                            <form.SubmitButton
                                loading={replaceNodes.isPending}
                                id={`${id}-save`}
                                label="保存草稿"
                                pendingLabel="保存中…"
                                variant="outline"
                                disabled={readOnly || replaceNodes.isPending}
                            />
                        </form.AppForm>
                        {publishAction}
                    </div>
                </div>
            </form>
            <form.Subscribe
                selector={(state) =>
                    [state.values.copyRules, state.values.nodes] as const
                }
            >
                {([rules, nodes]) => (
                    <CopyRuleEditor
                        id={`${id}-copies`}
                        open={copySettings !== null}
                        onOpenChange={(open) => {
                            if (!open) setCopySettings(null)
                        }}
                        targetNodeId={copySettings?.nodeClientId ?? null}
                        returnFocus={copyReturnFocus}
                        rules={rules}
                        nodes={nodes}
                        documentType={detail.document_type}
                        disabled={readOnly || replaceNodes.isPending}
                        onChange={(next) => {
                            form.setFieldValue("copyRules", next)
                            setSaveState("idle")
                        }}
                    />
                )}
            </form.Subscribe>
            <CopyFlowDialog
                id={`${id}-copy-dialog`}
                targetType={detail.document_type}
                open={copyOpen}
                onOpenChange={setCopyOpen}
                replacesNodes={form.state.values.nodes.length > 0}
                onCopy={(source) => {
                    const nodes = copyDraftNodes(source.nodes)
                    form.setFieldValue("nodes", nodes)
                    form.setFieldValue(
                        "copyRules",
                        seedCopyRules(source, nodes),
                    )
                    setSelectedNodeId(nodes[0]?.client_id ?? null)
                    setSaveState("idle")
                    setSubmitError(null)
                    toast.add({
                        title: "流程已带入",
                        description: "确认审批人后保存草稿。",
                        type: "success",
                    })
                }}
            />
        </>
    )
}

function DirtyObserver({
    dirty,
    onChange,
}: {
    dirty: boolean
    onChange?: (dirty: boolean) => void
}) {
    React.useEffect(() => {
        onChange?.(dirty)
        const warn = (event: BeforeUnloadEvent) => {
            event.preventDefault()
            event.returnValue = ""
        }
        if (dirty) window.addEventListener("beforeunload", warn)
        return () => {
            onChange?.(false)
            window.removeEventListener("beforeunload", warn)
        }
    }, [dirty, onChange])
    return null
}
