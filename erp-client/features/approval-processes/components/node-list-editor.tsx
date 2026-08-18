"use client"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"

import { canMutateNodeStructure, emptyEditorNode } from "../draft-nodes"
import { nodePurposeLabel } from "../labels"
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
            {nodes.map((node, index) => {
                const lockedPurpose =
                    !canMutateNodeStructure(node) || node.unsaved_purpose_slot
                return (
                    <div
                        key={node.client_id}
                        className="flex flex-col gap-3 rounded-lg border border-border/70 p-3"
                        data-testid={`approval-node-${index}`}
                        data-locked={lockedPurpose ? "true" : "false"}
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <p className="text-sm font-medium">
                                第 {index + 1} 个审批节点
                                {lockedPurpose ? (
                                    <span className="ml-2 text-xs text-muted-foreground">
                                        {nodePurposeLabel(node.node_purpose)}
                                    </span>
                                ) : null}
                            </p>
                            <div className="flex flex-wrap gap-2">
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={readOnly || index === 0}
                                    onClick={() => move(index, -1)}
                                >
                                    上移
                                </Button>
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={
                                        readOnly || index === nodes.length - 1
                                    }
                                    onClick={() => move(index, 1)}
                                >
                                    下移
                                </Button>
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    disabled={
                                        readOnly ||
                                        !canMutateNodeStructure(node)
                                    }
                                    onClick={() =>
                                        onChange(
                                            nodes.filter(
                                                (_, current) =>
                                                    current !== index,
                                            ),
                                        )
                                    }
                                >
                                    删除
                                </Button>
                            </div>
                        </div>
                        <div className="grid gap-3 md:grid-cols-2">
                            <div className="flex flex-col gap-1.5">
                                <Label htmlFor={`${node.client_id}-name`}>
                                    节点名称
                                </Label>
                                <Input
                                    id={`${node.client_id}-name`}
                                    value={node.node_name}
                                    disabled={readOnly}
                                    onChange={(event) =>
                                        update(index, {
                                            node_name: event.target.value,
                                        })
                                    }
                                />
                            </div>
                            <div className="flex flex-col gap-1.5">
                                <Label>审批人</Label>
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
                                            assignee_name: assignee?.name ?? "",
                                        })
                                    }
                                />
                            </div>
                        </div>
                        {documentType === "sales_order" && lockedPurpose ? (
                            <p className="text-xs text-muted-foreground">
                                采购确认用途由系统保持，只可改名称、顺序和审批人，不可删除或改用途。
                            </p>
                        ) : null}
                    </div>
                )
            })}
            <Button
                type="button"
                variant="outline"
                disabled={readOnly}
                onClick={() => onChange([...nodes, emptyEditorNode()])}
            >
                增加节点
            </Button>
        </div>
    )
}
