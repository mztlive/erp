"use client"

import { type ReactNode } from "react"
import { CircleHelpIcon } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { Item, ItemContent, ItemGroup } from "@/components/ui/item"
import {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverHeader,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { Separator } from "@/components/ui/separator"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"

import type { WorkspaceWorkItem } from "../types"
import { isBlockedWorkItem } from "../lib/work-item"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import { WorkspacePaneActions } from "./workspace-pane-actions"

const PRIORITY_LABEL: Record<number, string> = {
    1: "紧急",
    2: "高",
    3: "普通",
    4: "低",
}

/**
 * 作业面标题栏已内嵌问号入口的任务类型。其余类型在详情右上角补同一个入口。
 */
const INLINE_CONTEXT_HELP_TYPES = new Set([
    "DOCUMENT_APPROVAL",
    "APPROVAL_INSTANCE",
    "FULFILLMENT_OPERATION",
    "SUPPLIER_PAYMENT_EXECUTION",
    "SALES_INVOICE_EXECUTION",
    "CUSTOMER_ACCEPTANCE_REGISTRATION",
    "PROCUREMENT_ORDER_CREATION",
])

function responsibilityLabel(item: WorkspaceWorkItem): string {
    return [
        item.ownerRoleLabel,
        item.ownerUserLabel,
        item.ownerOrganizationLabel,
    ]
        .filter((value) => value.trim())
        .join(" · ")
}

/** 说明弹层里的一条到达原因、影响或下一步，用浅底卡片和正文分开。 */
function ContextFact({ label, value }: { label: string; value: string }) {
    return (
        <Item variant="muted" size="sm">
            <ItemContent>
                <span className="text-xs text-muted-foreground">{label}</span>
                <span className="text-pretty text-sm">{value}</span>
            </ItemContent>
        </Item>
    )
}

/**
 * 标题栏已把问号放在打开原单据或页头动作旁边时，不再额外叠一层入口。
 *
 * @param item 当前工作台任务
 * @returns 标题栏已内嵌问号则为 true
 */
export function workspaceTaskHasInlineContextHelp(
    item: Pick<WorkspaceWorkItem, "workItemType">,
): boolean {
    return INLINE_CONTEXT_HELP_TYPES.has(item.workItemType)
}

/**
 * 任务说明问号。点开后展示统一任务上下文，不单独占一块说明区。
 *
 * @param item 当前工作台任务，字段来自服务端 WorkItem 投影
 */
export function WorkspaceTaskContextHelp({
    item,
}: {
    item: WorkspaceWorkItem
}) {
    const blocked = isBlockedWorkItem(item)
    const overdue = item.dueBucket === "overdue"
    const priorityLabel = PRIORITY_LABEL[item.priority] ?? `P${item.priority}`

    return (
        <Popover>
            <PopoverTrigger
                id={`workspace-task-context-help-trigger-${toAutomationIdSegment(item.workItemId)}`}
                render={
                    <Button
                        id={`workspace-task-context-help-trigger-${toAutomationIdSegment(item.workItemId)}`}
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        aria-label="任务说明"
                        data-testid={`work-item-task-context-${item.workItemId}`}
                    />
                }
            >
                <CircleHelpIcon aria-hidden="true" />
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="w-80 sm:w-96"
                aria-label="任务责任与处理要求"
            >
                <PopoverHeader className="gap-2">
                    <div className="flex items-start justify-between gap-3">
                        <div className="flex min-w-0 flex-col gap-1">
                            <PopoverTitle>任务说明</PopoverTitle>
                            <PopoverDescription>
                                {item.workItemTypeLabel}
                            </PopoverDescription>
                        </div>
                        <StatusBadge
                            label={
                                blocked
                                    ? "处理受阻"
                                    : overdue
                                      ? "已超期"
                                      : "待处理"
                            }
                            tone={
                                blocked
                                    ? "warning"
                                    : overdue
                                      ? "destructive"
                                      : "info"
                            }
                        />
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                        <WorkspaceDocumentBadge item={item} />
                        <span className="text-xs text-muted-foreground">
                            优先级：{priorityLabel}
                        </span>
                    </div>
                </PopoverHeader>

                <ItemGroup className="gap-2">
                    <ContextFact label="为什么到你" value={item.reasonLabel} />
                    <ContextFact
                        label="不处理的影响"
                        value={item.impactSummary}
                    />
                    <ContextFact
                        label="现在应做什么"
                        value={item.nextActionHint}
                    />
                </ItemGroup>

                <Separator />

                <DescriptionList
                    columns="two"
                    className="grid-cols-2 gap-x-4 gap-y-3"
                >
                    <DescriptionItem className="col-span-2">
                        <DescriptionTerm>当前责任</DescriptionTerm>
                        <DescriptionDetails>
                            {responsibilityLabel(item) || "责任人待确认"}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>进入工作台</DescriptionTerm>
                        <DescriptionDetails>
                            {item.enteredAtLabel}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>处理时限</DescriptionTerm>
                        <DescriptionDetails>
                            {item.dueAt ? item.dueAtLabel : "未设置截止时间"}
                        </DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>

                {item.actionBlockers.length > 0 ? (
                    <Alert variant="warning">
                        <AlertTitle>当前处理受阻</AlertTitle>
                        <AlertDescription>
                            <ul className="flex list-disc flex-col gap-1 pl-4">
                                {item.actionBlockers.map((blocker, index) => (
                                    <li
                                        key={`${blocker.action}:${blocker.code}:${index}`}
                                    >
                                        {blocker.message}
                                    </li>
                                ))}
                            </ul>
                        </AlertDescription>
                    </Alert>
                ) : null}
            </PopoverContent>
        </Popover>
    )
}

/**
 * 作业面标题栏右侧动作。问号固定放在打开原单据等图标旁边。
 *
 * @param item 当前工作台任务
 * @param children 查看单据、打开原单据、转交等动作；无额外动作时只渲染问号
 */
export function WorkspaceTaskHeaderActions({
    item,
    children,
}: {
    item: WorkspaceWorkItem
    children?: ReactNode
}) {
    return (
        <div className="flex shrink-0 items-center gap-1">
            {children}
            <WorkspaceTaskContextHelp item={item} />
            <WorkspacePaneActions />
        </div>
    )
}
