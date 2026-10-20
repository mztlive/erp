"use client"

import * as React from "react"
import { CopyIcon, PlusIcon } from "lucide-react"
import { DraftSaveIndicator } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
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
import { DefinitionFlowchart } from "./definition-flowchart"
import { NodeListEditor } from "./node-list-editor"

const INCOMPLETE_DRAFT_MESSAGE = "请补全每个节点的名称和审批人后再保存。"

/** 流程图负责选择与顺序，属性栏只编辑当前节点。 */
export function DefinitionEditor({
    detail,
    lockVersion,
    onLockVersionChange,
    onSaved,
    onDirtyChange,
    id = "governance-approval-processes-detail-editor",
}: {
    detail: DefinitionDetailView
    lockVersion: string
    onLockVersionChange: (next: string) => void
    onSaved?: (next: DefinitionDetailView) => void
    onDirtyChange?: (dirty: boolean) => void
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
    const readOnly = detail.status !== "DRAFT"
    const form = useAppForm({
        defaultValues: {
            name: detail.name,
            nodes: seedDraftNodes(detail.document_type, detail.nodes),
        },
        validators: {
            onChange: definitionEditorSchema,
            onSubmit: definitionEditorSchema,
        },
        onSubmitInvalid: () => {
            setSubmitError(INCOMPLETE_DRAFT_MESSAGE)
            setSaveState("failed")
            const invalid = form.state.values.nodes.find(
                (node) => !node.node_name.trim() || !node.assignee_user_id,
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
                )
                pendingCommandRef.current = command
                const next = await replaceNodes.mutateAsync(command)
                pendingCommandRef.current = null
                const nextNodes = seedDraftNodes(next.document_type, next.nodes)
                const selectedIndex = value.nodes.findIndex(
                    (node) => node.client_id === selectedNodeId,
                )
                form.reset({ name: next.name, nodes: nextNodes })
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
    const addNode = () => {
        if (form.state.values.nodes.length >= 20) return
        const node = emptyEditorNode()
        form.setFieldValue("nodes", [...form.state.values.nodes, node])
        setSelectedNodeId(node.client_id)
        setSaveState("idle")
    }
    return (
        <>
            <form
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
                <div className="space-y-5 p-4 sm:p-6">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <div>
                            <h2 className="text-base font-semibold">
                                {detail.name}
                            </h2>
                            <p className="mt-1 text-xs text-muted-foreground">
                                选择流程图中的节点，设置审批人及顺序。
                            </p>
                        </div>
                        <div className="flex flex-wrap items-center gap-2">
                            <Button
                                id={`${id}-copy-flow`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={readOnly || replaceNodes.isPending}
                                onClick={() => setCopyOpen(true)}
                            >
                                <CopyIcon data-icon="inline-start" />
                                从其它流程复制
                            </Button>
                            <form.Subscribe
                                selector={(state) => state.values.nodes.length}
                            >
                                {(count) => (
                                    <Button
                                        id={`${id}-add-node`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={
                                            readOnly ||
                                            replaceNodes.isPending ||
                                            count >= 20
                                        }
                                        title={
                                            count >= 20
                                                ? "最多配置 20 个审批节点"
                                                : undefined
                                        }
                                        onClick={addNode}
                                    >
                                        <PlusIcon data-icon="inline-start" />
                                        添加节点
                                    </Button>
                                )}
                            </form.Subscribe>
                        </div>
                    </div>
                    {submitError && (
                        <Alert variant="destructive">
                            <AlertTitle>保存失败</AlertTitle>
                            <AlertDescription>{submitError}</AlertDescription>
                        </Alert>
                    )}
                    <form.Subscribe selector={(state) => state.values.nodes}>
                        {(nodes) => (
                            <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
                                <DefinitionFlowchart
                                    id={`${id}-flow`}
                                    nodes={nodes}
                                    selectedClientId={
                                        nodes.find(
                                            (node) =>
                                                node.client_id ===
                                                selectedNodeId,
                                        )?.client_id ?? nodes[0]?.client_id
                                    }
                                    onSelect={(clientId) => {
                                        setSelectedNodeId(clientId)
                                        if (
                                            window.matchMedia(
                                                "(max-width: 1023px)",
                                            ).matches
                                        ) {
                                            document
                                                .getElementById(
                                                    `${id}-settings`,
                                                )
                                                ?.scrollIntoView({
                                                    behavior: "smooth",
                                                    block: "start",
                                                })
                                        }
                                    }}
                                    onAdd={
                                        readOnly ||
                                        replaceNodes.isPending ||
                                        nodes.length >= 20
                                            ? undefined
                                            : addNode
                                    }
                                />
                                <div
                                    id={`${id}-settings`}
                                    className="min-w-0 scroll-mt-4 space-y-4 lg:sticky lg:top-4"
                                >
                                    <NodeListEditor
                                        id={`${id}-nodes`}
                                        documentType={detail.document_type}
                                        nodes={nodes}
                                        readOnly={
                                            readOnly || replaceNodes.isPending
                                        }
                                        selectedClientId={selectedNodeId}
                                        onChange={(next) => {
                                            const activeId =
                                                nodes.find(
                                                    (node) =>
                                                        node.client_id ===
                                                        selectedNodeId,
                                                )?.client_id ??
                                                nodes[0]?.client_id
                                            const oldIndex = nodes.findIndex(
                                                (node) =>
                                                    node.client_id === activeId,
                                            )
                                            setSelectedNodeId(
                                                next.find(
                                                    (node) =>
                                                        node.client_id ===
                                                        activeId,
                                                )?.client_id ??
                                                    next[
                                                        Math.max(
                                                            0,
                                                            Math.min(
                                                                oldIndex,
                                                                next.length - 1,
                                                            ),
                                                        )
                                                    ]?.client_id ??
                                                    null,
                                            )
                                            form.setFieldValue("nodes", next)
                                            setSaveState("idle")
                                        }}
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
                            </div>
                        )}
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
                                        !node.assignee_user_id,
                                ).length
                                return (
                                    <span className="text-xs text-muted-foreground">
                                        {nodes.length === 0
                                            ? "请添加至少一个审批节点"
                                            : incomplete
                                              ? `${incomplete} 个节点待补全，点击流程图继续配置`
                                              : dirty
                                                ? "有未保存的修改，请保存后再发布"
                                                : "草稿未发布，不影响当前生效流程"}
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
                    <form.AppForm>
                        <form.SubmitButton
                            loading={replaceNodes.isPending}
                            id={`${id}-save`}
                            label="保存草稿"
                            pendingLabel="保存中…"
                            disabled={readOnly || replaceNodes.isPending}
                        />
                    </form.AppForm>
                </div>
            </form>
            <CopyFlowDialog
                id={`${id}-copy-dialog`}
                targetType={detail.document_type}
                open={copyOpen}
                onOpenChange={setCopyOpen}
                replacesNodes={form.state.values.nodes.length > 0}
                onCopy={(source) => {
                    const nodes = copyDraftNodes(source.nodes)
                    form.setFieldValue("nodes", nodes)
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
