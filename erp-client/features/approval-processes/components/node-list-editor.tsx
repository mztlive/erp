"use client"

import {
    ChevronDownIcon,
    ChevronUpIcon,
    PlusIcon,
    Trash2Icon,
} from "lucide-react"

import { surfaceInsetClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { cn } from "@/lib/utils"

import { emptyEditorNode } from "../draft-nodes"
import { REJECT_RESTART_COPY } from "../labels"
import type { DocumentType, EditorNode, EligibleAssignee } from "../types"
import { AssigneeCombobox } from "./assignee-combobox"

/**
 * 线性节点列表：只允许增删、排序、改名称和选择单人审批人。
 */
export function NodeListEditor({
    documentType,
    nodes,
    readOnly,
    onChange,
}: {
    documentType: DocumentType
    nodes: EditorNode[]
    readOnly: boolean
    onChange: (nodes: EditorNode[]) => void
}) {
    const move = (index: number, offset: number) => {
        const target = index + offset
        if (target < 0 || target >= nodes.length) return
        const next = [...nodes]
        const current = next[index]
        const swapped = next[target]
        if (!current || !swapped) return
        next[index] = swapped
        next[target] = current
        onChange(next)
    }

    const update = (index: number, patch: Partial<EditorNode>) => {
        onChange(
            nodes.map((node, current) =>
                current === index ? { ...node, ...patch } : node,
            ),
        )
    }

    return (
        <div className="flex flex-col gap-3">
            <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                <h3 className="text-sm font-medium">审批节点</h3>
                <p className="text-xs text-muted-foreground">
                    {nodes.length} 个节点 · {REJECT_RESTART_COPY}
                </p>
            </div>
            <ol className="flex flex-col">
                {nodes.map((node, index) => {
                    const showConnector = index < nodes.length - 1 || !readOnly
                    return (
                        <li
                            key={node.client_id}
                            className="relative flex gap-3"
                            data-testid={`approval-node-${index}`}
                        >
                            <div className="relative flex w-7 shrink-0 flex-col items-center">
                                <span
                                    className={cn(
                                        "num relative z-10 flex size-7 items-center justify-center rounded-full text-xs font-medium",
                                        node.assignee_user_id
                                            ? "bg-accent text-foreground ring-1 ring-primary/25"
                                            : "bg-muted text-muted-foreground ring-1 ring-foreground/10",
                                    )}
                                    aria-hidden="true"
                                >
                                    {index + 1}
                                </span>
                                {showConnector ? (
                                    <span
                                        className="mt-1 w-px flex-1 bg-grid"
                                        aria-hidden="true"
                                    />
                                ) : null}
                            </div>
                            <div
                                className={cn(
                                    surfaceInsetClassName,
                                    "mb-3 min-w-0 flex-1 p-3",
                                    index === nodes.length - 1 &&
                                        readOnly &&
                                        "mb-0",
                                )}
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <p className="text-sm font-medium">
                                        第 {index + 1} 个审批节点
                                    </p>
                                    <div className="flex flex-wrap gap-1">
                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="ghost"
                                            disabled={readOnly || index === 0}
                                            onClick={() => move(index, -1)}
                                        >
                                            <ChevronUpIcon data-icon="inline-start" />
                                            上移
                                        </Button>
                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="ghost"
                                            disabled={
                                                readOnly ||
                                                index === nodes.length - 1
                                            }
                                            onClick={() => move(index, 1)}
                                        >
                                            <ChevronDownIcon data-icon="inline-start" />
                                            下移
                                        </Button>
                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="ghost"
                                            disabled={readOnly}
                                            onClick={() =>
                                                onChange(
                                                    nodes.filter(
                                                        (_, current) =>
                                                            current !== index,
                                                    ),
                                                )
                                            }
                                        >
                                            <Trash2Icon data-icon="inline-start" />
                                            删除
                                        </Button>
                                    </div>
                                </div>
                                <div className="mt-3 grid gap-3 md:grid-cols-2">
                                    <div className="flex flex-col gap-1.5">
                                        <Label
                                            htmlFor={`${node.client_id}-name`}
                                        >
                                            节点名称<span className="text-destructive">*</span>
                                        </Label>
                                        <Input
                                            id={`${node.client_id}-name`}
                                            value={node.node_name}
                                            disabled={readOnly}
                                            placeholder="例如：财务复核"
                                            onChange={(event) =>
                                                update(index, {
                                                    node_name:
                                                        event.target.value,
                                                })
                                            }
                                        />
                                    </div>
                                    <div className="flex flex-col gap-1.5">
                                        <Label>审批人<span className="text-destructive">*</span></Label>
                                        <AssigneeCombobox
                                            documentType={documentType}
                                            value={node.assignee_user_id}
                                            selectedName={node.assignee_name}
                                            disabled={readOnly}
                                            onChange={(
                                                assignee: EligibleAssignee | null,
                                            ) =>
                                                update(index, {
                                                    assignee_user_id:
                                                        assignee?.user_id ?? "",
                                                    assignee_name:
                                                        assignee?.name ?? "",
                                                })
                                            }
                                        />
                                    </div>
                                </div>
                            </div>
                        </li>
                    )
                })}
                {readOnly ? null : (
                    <li className="flex gap-3">
                        <div className="flex w-7 shrink-0 justify-center">
                            <span
                                className="flex size-7 items-center justify-center rounded-full border border-dashed border-border text-muted-foreground"
                                aria-hidden="true"
                            >
                                <PlusIcon className="size-3.5" />
                            </span>
                        </div>
                        <Button
                            type="button"
                            variant="secondary"
                            disabled={readOnly}
                            className="h-9 flex-1 rounded-lg border border-dashed border-border bg-transparent shadow-none hover:border-primary/40 hover:bg-accent/50"
                            onClick={() =>
                                onChange([...nodes, emptyEditorNode()])
                            }
                        >
                            <PlusIcon data-icon="inline-start" />
                            增加节点
                        </Button>
                    </li>
                )}
            </ol>
        </div>
    )
}
