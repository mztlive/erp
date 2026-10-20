"use client"

import * as React from "react"
import {
    BellIcon,
    ChevronDownIcon,
    PlusIcon,
    Trash2Icon,
    XIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import {
    Select,
    SelectContent,
    SelectGroup,
    SelectItem,
    SelectLabel,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetFooter,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { AssigneeCombobox } from "./assignee-combobox"
import { COPY_TRIGGER_LABELS } from "../copy-rules"
import type {
    CopyTrigger,
    DefinitionDetailView,
    DocumentType,
    EditorCopyRule,
    EditorNode,
} from "../types"

type CopyRuleEditorProps = {
    id: string
    rules: EditorCopyRule[]
    nodes: EditorNode[]
    documentType: DocumentType
    disabled: boolean
    onChange: (rules: EditorCopyRule[]) => void
}

function ruleKey(rule: Pick<EditorCopyRule, "trigger" | "nodeClientId">) {
    return `${rule.trigger}-${rule.nodeClientId ?? "process"}`
}

export function CopyRuleEditor({
    open,
    onOpenChange,
    targetNodeId,
    returnFocus,
    ...props
}: CopyRuleEditorProps & {
    open: boolean
    onOpenChange: (open: boolean) => void
    targetNodeId: string | null
    returnFocus: React.RefObject<HTMLElement | null>
}) {
    return (
        <Sheet open={open} onOpenChange={onOpenChange}>
            <SheetContent
                size="preview"
                closeButtonId={`${props.id}-close`}
                finalFocus={returnFocus}
            >
                <SheetHeader>
                    <SheetTitle>抄送设置</SheetTitle>
                    <SheetDescription>
                        抄送人仅查看资料，不参与审批。
                    </SheetDescription>
                </SheetHeader>
                {open && (
                    <CopyRuleList {...props} targetNodeId={targetNodeId} />
                )}
                <SheetFooter className="border-t">
                    <p className="mr-auto w-full text-xs leading-5 text-muted-foreground">
                        关闭面板保留修改，请在页面点击「保存草稿」。
                    </p>
                    <Button
                        id={`${props.id}-done`}
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        返回流程
                    </Button>
                </SheetFooter>
            </SheetContent>
        </Sheet>
    )
}

function CopyRuleList({
    id,
    rules,
    nodes,
    documentType,
    disabled,
    onChange,
    targetNodeId,
}: CopyRuleEditorProps & { targetNodeId: string | null }) {
    const [activeKey, setActiveKey] = React.useState<string | null>(
        targetNodeId
            ? ruleKey({ trigger: "NODE_APPROVED", nodeClientId: targetNodeId })
            : null,
    )
    const [adding, setAdding] = React.useState(false)
    // 仅记录当前打开的选择控件；规则与接收人始终由主表单持有。
    const [pendingKey, setPendingKey] = React.useState<string | null>(null)
    const slots = [
        ...(
            ["STARTED", "APPROVED", "REJECTED", "CANCELLED"] as CopyTrigger[]
        ).map((trigger) => ({
            trigger,
            nodeClientId: null,
            label: COPY_TRIGGER_LABELS[trigger],
        })),
        ...nodes.map((node, index) => ({
            trigger: "NODE_APPROVED" as const,
            nodeClientId: node.client_id,
            label: `${node.node_name.trim() || `第 ${index + 1} 个节点（未命名）`}通过后`,
        })),
    ]
    const available = slots.filter(
        (slot) => !rules.some((rule) => ruleKey(rule) === ruleKey(slot)),
    )
    const pending = available.find((slot) => ruleKey(slot) === pendingKey)
    const update = (
        rule: EditorCopyRule,
        recipients: EditorCopyRule["recipients"],
    ) => {
        onChange(
            rules.flatMap((item) =>
                item === rule
                    ? recipients.length
                        ? [{ ...rule, recipients }]
                        : []
                    : [item],
            ),
        )
    }
    const activeRef = React.useRef<HTMLDivElement | null>(null)
    React.useEffect(() => {
        if (targetNodeId)
            activeRef.current?.scrollIntoView({ block: "nearest" })
    }, [targetNodeId])

    return (
        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-7 pb-6">
            {rules.length ? (
                <>
                    <p className="text-xs text-muted-foreground">
                        已配置 {rules.length} 个抄送时机
                    </p>
                    <div className="space-y-3">
                        {slots.map((slot) => {
                            const key = ruleKey(slot)
                            const rule = rules.find(
                                (item) => ruleKey(item) === key,
                            )
                            if (!rule) return null
                            const ruleId = `${id}-${toAutomationIdSegment(key)}`
                            const expanded = activeKey === key
                            return (
                                <div
                                    key={key}
                                    ref={expanded ? activeRef : undefined}
                                    className="overflow-hidden rounded-lg border bg-card"
                                >
                                    <button
                                        id={`${ruleId}-toggle`}
                                        type="button"
                                        className="flex w-full items-start gap-3 p-3 text-left hover:bg-muted/40 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
                                        aria-expanded={expanded}
                                        aria-controls={`${ruleId}-content`}
                                        onClick={() =>
                                            setActiveKey(expanded ? null : key)
                                        }
                                    >
                                        <div className="min-w-0 flex-1 space-y-1">
                                            <p className="break-words text-sm font-medium">
                                                {slot.label}
                                            </p>
                                            <p className="break-words text-xs leading-5 text-muted-foreground">
                                                {rule.recipients
                                                    .slice(0, 3)
                                                    .map(
                                                        (person) => person.name,
                                                    )
                                                    .join("、")}
                                                {rule.recipients.length > 3
                                                    ? ` 等 ${rule.recipients.length} 人`
                                                    : ""}
                                            </p>
                                        </div>
                                        <ChevronDownIcon
                                            className={`mt-1 size-4 shrink-0 text-muted-foreground transition-transform ${expanded ? "rotate-180" : ""}`}
                                        />
                                    </button>
                                    <div
                                        id={`${ruleId}-content`}
                                        hidden={!expanded}
                                        className="space-y-3 border-t p-3"
                                    >
                                        {expanded && (
                                            <>
                                                <div className="flex flex-wrap gap-2">
                                                    {rule.recipients.map(
                                                        (person) => (
                                                            <Button
                                                                key={
                                                                    person.user_id
                                                                }
                                                                id={`${ruleId}-remove-${toAutomationIdSegment(person.user_id)}`}
                                                                type="button"
                                                                variant="secondary"
                                                                size="sm"
                                                                disabled={
                                                                    disabled
                                                                }
                                                                className="h-auto min-h-7 max-w-full whitespace-normal text-left"
                                                                aria-label={`移除抄送人${person.name}`}
                                                                onClick={() =>
                                                                    update(
                                                                        rule,
                                                                        rule.recipients.filter(
                                                                            (
                                                                                item,
                                                                            ) =>
                                                                                item.user_id !==
                                                                                person.user_id,
                                                                        ),
                                                                    )
                                                                }
                                                            >
                                                                {person.name}
                                                                <XIcon className="size-3" />
                                                            </Button>
                                                        ),
                                                    )}
                                                </div>
                                                <AssigneeCombobox
                                                    id={`${ruleId}-add`}
                                                    copies
                                                    documentType={documentType}
                                                    value=""
                                                    selectedName=""
                                                    placeholder="添加抄送人"
                                                    excludedIds={rule.recipients.map(
                                                        (person) =>
                                                            person.user_id,
                                                    )}
                                                    disabled={
                                                        disabled ||
                                                        rule.recipients
                                                            .length >= 50
                                                    }
                                                    onChange={(person) => {
                                                        if (person)
                                                            update(rule, [
                                                                ...rule.recipients,
                                                                person,
                                                            ])
                                                    }}
                                                />
                                                <div className="flex items-center justify-between gap-2">
                                                    <span className="text-xs text-muted-foreground">
                                                        {rule.recipients.length}{" "}
                                                        / 50 人
                                                    </span>
                                                    <Button
                                                        id={`${ruleId}-delete`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="sm"
                                                        disabled={disabled}
                                                        onClick={() => {
                                                            update(rule, [])
                                                            setActiveKey(null)
                                                        }}
                                                    >
                                                        <Trash2Icon data-icon="inline-start" />
                                                        删除规则
                                                    </Button>
                                                </div>
                                            </>
                                        )}
                                    </div>
                                </div>
                            )
                        })}
                    </div>
                </>
            ) : (
                !adding && (
                    <div className="rounded-lg border border-dashed px-4 py-8 text-center">
                        <BellIcon className="mx-auto mb-3 size-6 text-muted-foreground" />
                        <p className="text-sm font-medium">尚未设置抄送</p>
                        <p className="mt-2 text-xs leading-5 text-muted-foreground">
                            按需选择通知时机和接收人。
                        </p>
                    </div>
                )
            )}
            {adding ? (
                <div className="space-y-3 rounded-lg border bg-muted/20 p-3">
                    <div className="flex items-center justify-between gap-2">
                        <h3 className="text-sm font-medium">添加抄送规则</h3>
                        <Button
                            id={`${id}-cancel-add`}
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            aria-label="取消添加规则"
                            onClick={() => {
                                setAdding(false)
                                setPendingKey(null)
                            }}
                        >
                            <XIcon />
                        </Button>
                    </div>
                    <div className="space-y-2">
                        <Label htmlFor={`${id}-trigger`}>触发时机</Label>
                        <Select
                            value={pendingKey}
                            onValueChange={setPendingKey}
                            disabled={disabled}
                        >
                            <SelectTrigger
                                id={`${id}-trigger`}
                                className="w-full min-w-0"
                            >
                                <SelectValue placeholder="选择抄送时机">
                                    {pending?.label}
                                </SelectValue>
                            </SelectTrigger>
                            <SelectContent alignItemWithTrigger={false}>
                                {[false, true].map((nodeEvent) => (
                                    <SelectGroup key={String(nodeEvent)}>
                                        <SelectLabel>
                                            {nodeEvent
                                                ? "节点事件"
                                                : "流程事件"}
                                        </SelectLabel>
                                        {available
                                            .filter(
                                                (slot) =>
                                                    Boolean(
                                                        slot.nodeClientId,
                                                    ) === nodeEvent,
                                            )
                                            .map((slot) => (
                                                <SelectItem
                                                    key={ruleKey(slot)}
                                                    idPrefix={`${id}-trigger`}
                                                    value={ruleKey(slot)}
                                                >
                                                    <span className="whitespace-normal break-words">
                                                        {slot.label}
                                                    </span>
                                                </SelectItem>
                                            ))}
                                    </SelectGroup>
                                ))}
                            </SelectContent>
                        </Select>
                    </div>
                    {pending && (
                        <div className="space-y-2">
                            <p className="text-sm font-medium">抄送人</p>
                            <AssigneeCombobox
                                id={`${id}-new-recipient`}
                                copies
                                documentType={documentType}
                                value=""
                                selectedName=""
                                placeholder="选择抄送人"
                                disabled={disabled}
                                onChange={(person) => {
                                    if (!person) return
                                    onChange([
                                        ...rules,
                                        {
                                            trigger: pending.trigger,
                                            nodeClientId: pending.nodeClientId,
                                            recipients: [person],
                                        },
                                    ])
                                    setActiveKey(ruleKey(pending))
                                    setAdding(false)
                                    setPendingKey(null)
                                }}
                            />
                        </div>
                    )}
                </div>
            ) : (
                <Button
                    id={`${id}-add-rule`}
                    type="button"
                    variant="outline"
                    className="w-full"
                    disabled={disabled || !available.length}
                    onClick={() => {
                        setAdding(true)
                        setActiveKey(null)
                    }}
                >
                    <PlusIcon data-icon="inline-start" />
                    {available.length ? "添加抄送规则" : "全部时机均已配置"}
                </Button>
            )}
        </div>
    )
}

export function CopyRuleSummary({
    detail,
    embedded = false,
}: {
    detail: DefinitionDetailView
    embedded?: boolean
}) {
    return (
        <section
            className={cn(
                "min-w-0 space-y-3",
                !embedded && "rounded-lg border p-4",
            )}
            aria-label="抄送规则"
        >
            <h3 className="flex items-center gap-2 text-sm font-medium">
                <BellIcon
                    aria-hidden
                    className="size-4 text-muted-foreground"
                />
                抄送设置
                <span className="ml-auto text-xs font-normal text-muted-foreground">
                    {detail.copy_rules?.length
                        ? `${detail.copy_rules.length} 条规则`
                        : "未设置"}
                </span>
            </h3>
            {detail.copy_rules?.length ? (
                <dl className="space-y-3">
                    {detail.copy_rules.map((rule) => (
                        <div key={`${rule.trigger}-${rule.node_order}`}>
                            <dt className="break-words text-xs text-muted-foreground">
                                {rule.node_order
                                    ? `${detail.nodes.find((n) => n.display_order === rule.node_order)?.node_name ?? "节点"}通过后`
                                    : COPY_TRIGGER_LABELS[rule.trigger]}
                            </dt>
                            <dd className="mt-1.5 flex flex-wrap gap-1.5">
                                {rule.recipient_user_ids.map((id) => (
                                    <span
                                        key={id}
                                        className="max-w-full break-words rounded-md bg-muted px-2 py-1 text-xs"
                                    >
                                        {detail.copy_recipient_names?.[id] ??
                                            "待确认人员"}
                                    </span>
                                ))}
                            </dd>
                        </div>
                    ))}
                </dl>
            ) : (
                <p className="text-xs text-muted-foreground">
                    此版本不发送抄送通知。
                </p>
            )}
        </section>
    )
}
