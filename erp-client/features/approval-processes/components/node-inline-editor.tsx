"use client"

import {
    BellIcon,
    ChevronDownIcon,
    ChevronUpIcon,
    Trash2Icon,
    XIcon,
} from "lucide-react"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    AssigneeSource,
    DocumentType,
    EditorNode,
    EligibleAssignee,
} from "../types"
import { nodePeople } from "./flow-canvas-model"
import { PersonPicker } from "./node-assignee-group"

/** 右侧步骤检查器；字段变更仍写入父级草稿表单。 */
export function NodeInlineEditor({
    nodes,
    readOnly,
    documentType,
    onChange,
    node,
    copyCount = 0,
    onConfigureCopies,
    id = "governance-approval-processes-detail-editor-nodes",
}: {
    nodes: EditorNode[]
    readOnly: boolean
    documentType: DocumentType
    onChange: (nodes: EditorNode[]) => void
    node: EditorNode
    copyCount?: number
    onConfigureCopies?: () => void
    id?: string
}) {
    const index = nodes.findIndex((item) => item.client_id === node.client_id)
    const segment = toAutomationIdSegment(node.client_id)
    const people = nodePeople(node)
    const update = (patch: Partial<EditorNode>) =>
        onChange(
            nodes.map((item) =>
                item.client_id === node.client_id
                    ? { ...item, ...patch }
                    : item,
            ),
        )
    const updatePeople = (
        next: EligibleAssignee[],
        focusPerson = next[0]?.user_id,
    ) => {
        update({
            assignee_user_id: next[0]?.user_id ?? "",
            assignee_name: next[0]?.name ?? "",
            additional_assignees: next.slice(1),
        })
        const focusId = focusPerson
            ? `${id}-node-${segment}-person-${toAutomationIdSegment(focusPerson)}-trigger`
            : `${id}-node-${segment}-person-add-trigger`
        requestAnimationFrame(() =>
            document.getElementById(focusId)?.focus({ preventScroll: true }),
        )
    }
    const move = (offset: number) => {
        const next = [...nodes]
        const target = index + offset
        if (!next[target]) return
        ;[next[index], next[target]] = [next[target], node]
        onChange(next)
    }
    return (
        <div
            role="group"
            aria-label={`第 ${index + 1} 个节点设置`}
            data-testid={`approval-node-${index}`}
            className="space-y-5"
        >
            <div className="flex items-center justify-between gap-2">
                <div className="flex min-w-0 items-center gap-2">
                    <span className="num flex size-6 shrink-0 items-center justify-center rounded-md bg-muted text-xs font-medium text-muted-foreground">
                        {String(index + 1).padStart(2, "0")}
                    </span>
                    <h3 className="truncate text-sm font-medium">步骤设置</h3>
                </div>
                <div className="flex items-center gap-0.5">
                    <Button
                        id={`${id}-node-${segment}-move-up`}
                        type="button"
                        size="icon-sm"
                        variant="ghost"
                        aria-label="上移节点"
                        title="上移节点"
                        disabled={readOnly || index === 0}
                        onClick={() => move(-1)}
                    >
                        <ChevronUpIcon />
                    </Button>
                    <Button
                        id={`${id}-node-${segment}-move-down`}
                        type="button"
                        size="icon-sm"
                        variant="ghost"
                        aria-label="下移节点"
                        title="下移节点"
                        disabled={readOnly || index === nodes.length - 1}
                        onClick={() => move(1)}
                    >
                        <ChevronDownIcon />
                    </Button>
                    <Button
                        id={`${id}-node-${segment}-remove`}
                        type="button"
                        size="icon-sm"
                        variant="ghost"
                        className="text-muted-foreground hover:text-destructive"
                        aria-label="删除节点"
                        title="删除节点"
                        disabled={readOnly}
                        onClick={() =>
                            onChange(
                                nodes.filter(
                                    (item) => item.client_id !== node.client_id,
                                ),
                            )
                        }
                    >
                        <Trash2Icon />
                    </Button>
                </div>
            </div>
            <div className="space-y-4">
                <div className="space-y-2">
                    <Label htmlFor={`${id}-node-${segment}-name`}>
                        节点名称 <span className="text-destructive">*</span>
                    </Label>
                    <Input
                        id={`${id}-node-${segment}-name`}
                        value={node.node_name}
                        maxLength={64}
                        disabled={readOnly}
                        placeholder="例如：财务复核"
                        onChange={(event) =>
                            update({ node_name: event.target.value })
                        }
                    />
                </div>
                <div className="space-y-2">
                    <Label htmlFor={`${id}-node-${segment}-source`}>
                        审批人来源
                    </Label>
                    <OptionCombobox
                        id={`${id}-node-${segment}-source`}
                        disabled={readOnly}
                        value={node.assignee_source}
                        allowClear={false}
                        options={[
                            { value: "SPECIFIED", label: "指定人员" },
                            {
                                value: "APPLICANT_TEAM_LEADER",
                                label: "申请人的团队负责人",
                            },
                            {
                                value: "APPLICANT_DEPARTMENT_LEADER",
                                label: "申请人的部门负责人",
                            },
                        ]}
                        onValueChange={(value) => {
                            if (!value || value === node.assignee_source) return
                            const source = value as AssigneeSource
                            update({
                                assignee_source: source,
                                assignee_user_id: "",
                                additional_assignees: [],
                                assignee_name:
                                    source === "APPLICANT_TEAM_LEADER"
                                        ? "申请人的团队负责人"
                                        : source ===
                                            "APPLICANT_DEPARTMENT_LEADER"
                                          ? "申请人的部门负责人"
                                          : "",
                            })
                        }}
                    />
                </div>
                {node.assignee_source === "SPECIFIED" ? (
                    <div className="space-y-2">
                        <div className="flex items-center justify-between gap-2">
                            <Label>审批人</Label>
                            <span className="text-xs text-muted-foreground">
                                {people.length > 1 ? "任一人通过" : "单人审批"}
                            </span>
                        </div>
                        <div className="space-y-1.5">
                            {people.map((person) => (
                                <div
                                    key={person.user_id}
                                    className="flex items-center gap-1 rounded-lg border border-border bg-card px-2 py-1"
                                >
                                    <PersonPicker
                                        id={`${id}-node-${segment}-person-${toAutomationIdSegment(person.user_id)}`}
                                        documentType={documentType}
                                        people={people}
                                        person={person}
                                        disabled={readOnly}
                                        onChoose={(replacement) =>
                                            updatePeople(
                                                people.map((item) =>
                                                    item.user_id ===
                                                    person.user_id
                                                        ? replacement
                                                        : item,
                                                ),
                                                replacement.user_id,
                                            )
                                        }
                                    />
                                    <Button
                                        id={`${id}-node-${segment}-person-${toAutomationIdSegment(person.user_id)}-remove`}
                                        type="button"
                                        size="icon-xs"
                                        variant="ghost"
                                        className="shrink-0"
                                        aria-label={`移除审批人${person.name}`}
                                        disabled={readOnly}
                                        onClick={() =>
                                            updatePeople(
                                                people.filter(
                                                    (item) =>
                                                        item.user_id !==
                                                        person.user_id,
                                                ),
                                            )
                                        }
                                    >
                                        <XIcon />
                                    </Button>
                                </div>
                            ))}
                            <div className="flex rounded-lg border border-dashed border-border">
                                <PersonPicker
                                    id={`${id}-node-${segment}-person-add`}
                                    documentType={documentType}
                                    people={people}
                                    disabled={readOnly || people.length >= 50}
                                    onChoose={(person) =>
                                        updatePeople(
                                            [...people, person],
                                            person.user_id,
                                        )
                                    }
                                />
                            </div>
                        </div>
                        {people.length > 1 ? (
                            <p className="text-xs leading-5 text-muted-foreground">
                                任一人通过后进入下一步。
                            </p>
                        ) : null}
                    </div>
                ) : (
                    <p className="text-xs leading-5 text-muted-foreground">
                        提交时按申请人所属组织确定负责人；请先在“组织与人员”中配置负责人。审批开始后人员保持不变。
                    </p>
                )}
                {onConfigureCopies ? (
                    <Button
                        id={`${id}-node-${segment}-copies`}
                        type="button"
                        variant="outline"
                        className="w-full justify-start"
                        disabled={readOnly}
                        onClick={onConfigureCopies}
                    >
                        <BellIcon aria-hidden data-icon="inline-start" />
                        通过后抄送
                        <span className="ml-auto text-xs font-normal text-muted-foreground">
                            {copyCount ? `${copyCount} 人` : "未设置"}
                        </span>
                    </Button>
                ) : null}
            </div>
        </div>
    )
}
