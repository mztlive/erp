"use client"

import * as React from "react"
import {
    CheckIcon,
    CircleCheckIcon,
    CircleXIcon,
    LoaderCircleIcon,
    PlusIcon,
    Trash2Icon,
    TriangleAlertIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    Card,
    CardContent,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { GuardedBusinessAction } from "@/components/business/feedback"
import { taxAmountToneClass } from "@/components/business/values"
import { sequentialText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

type LineItemMode = "view" | "edit"
type CellAlignment = "start" | "center" | "end"

type EditableLineItemContext<TItem> = Readonly<{
    item: TItem
    rowId: string
    rowIndex: number
    mode: LineItemMode
    disabled: boolean
}>

interface EditableLineItemColumn<TItem> {
    /** 同一张表内稳定且唯一的列标识。 */
    readonly id: string
    readonly header: React.ReactNode
    /** 只读态内容。 */
    readonly renderValue: (
        context: EditableLineItemContext<TItem>,
    ) => React.ReactNode
    /** 编辑态字段插槽；通常由页面通过 TanStack Form 注入。 */
    readonly renderEditor?: (
        context: EditableLineItemContext<TItem>,
    ) => React.ReactNode
    readonly align?: CellAlignment
    readonly numeric?: boolean
}

interface EditableLineItemTableProps<TItem> extends Omit<
    React.ComponentPropsWithoutRef<"section">,
    "children"
> {
    readonly items: readonly TItem[]
    readonly columns: readonly EditableLineItemColumn<TItem>[]
    readonly getRowId: (item: TItem, rowIndex: number) => string
    readonly mode?: LineItemMode
    readonly disabled?: boolean
    readonly caption?: string
    readonly emptyContent?: React.ReactNode
    readonly footer?: React.ReactNode
    readonly addLabel?: string
    readonly addDisabledReason?: string
    readonly onAddItem?: () => void
    readonly onRemoveItem?: (
        item: TItem,
        rowId: string,
        rowIndex: number,
    ) => void
    readonly getRemoveDisabledReason?: (
        context: EditableLineItemContext<TItem>,
    ) => string | undefined
    readonly renderRowActions?: (
        context: EditableLineItemContext<TItem>,
    ) => React.ReactNode
    readonly getRowErrors?: (
        context: EditableLineItemContext<TItem>,
    ) => readonly React.ReactNode[]
}

type StickyTotalItem = Readonly<{
    id: string
    label: React.ReactNode
    value: React.ReactNode
    description?: React.ReactNode
}>

type StickyTotalBarProps = Omit<
    React.ComponentPropsWithoutRef<"aside">,
    "children"
> & {
    items: readonly StickyTotalItem[]
    note?: React.ReactNode
    actions?: React.ReactNode
    /** 左侧次级动作（如分步流程的"上一步"）；与 `actions` 分开渲染，不挤右侧主动作组。 */
    leftActions?: React.ReactNode
}

/** M5 编辑工作区统一的金额汇总与提交动作条。 */
function StickyTotalBar({
    items,
    note,
    actions,
    leftActions,
    className,
    ...props
}: StickyTotalBarProps) {
    return (
        <aside
            data-slot="sticky-total-bar"
            className={cn(
                "sticky bottom-3 z-20 rounded-2xl border border-border bg-background/95 p-3 shadow-lg backdrop-blur supports-[backdrop-filter]:bg-background/85",
                className,
            )}
            {...props}
        >
            <div className="flex flex-col gap-3 lg:flex-row lg:items-center">
                {leftActions ? (
                    <div className="flex shrink-0 flex-wrap gap-2">
                        {leftActions}
                    </div>
                ) : null}
                <div className="grid min-w-0 flex-1 gap-3 sm:grid-cols-3">
                    {items.map((item) => (
                        <div key={item.id} className="min-w-0">
                            <div className="text-xs text-muted-foreground">
                                {item.label}
                            </div>
                            <div
                                className={cn(
                                    "num mt-0.5 font-medium text-foreground",
                                    taxAmountToneClass(item.label),
                                )}
                            >
                                {item.value}
                            </div>
                            {item.description ? (
                                <div className="mt-0.5 text-xs text-muted-foreground">
                                    {item.description}
                                </div>
                            ) : null}
                        </div>
                    ))}
                </div>
                {note ? (
                    <div className="text-xs text-muted-foreground lg:max-w-sm">
                        {note}
                    </div>
                ) : null}
                {actions ? (
                    <div className="flex shrink-0 flex-wrap justify-end gap-2">
                        {actions}
                    </div>
                ) : null}
            </div>
        </aside>
    )
}

const alignmentClass: Record<CellAlignment, string> = {
    start: "text-left",
    center: "text-center",
    end: "text-right",
}

/**
 * 可编辑明细行的受控表格骨架。
 *
 * 组件不管理字段值、校验或提交；编辑器由页面的 TanStack Form 字段插槽注入。
 */
function EditableLineItemTable<TItem>({
    items,
    columns,
    getRowId,
    mode = "edit",
    disabled = false,
    caption = "业务明细",
    emptyContent = "暂无明细，可新增一行后继续填写。",
    footer,
    addLabel = "新增明细",
    addDisabledReason,
    onAddItem,
    onRemoveItem,
    getRemoveDisabledReason,
    renderRowActions,
    getRowErrors,
    className,
    ...props
}: EditableLineItemTableProps<TItem>) {
    const hasRowActions = Boolean(onRemoveItem || renderRowActions)
    const columnCount = columns.length + (hasRowActions ? 1 : 0)
    const canAdd = Boolean(onAddItem) && !disabled && !addDisabledReason

    /** 可编辑表单元格：容纳表单控件，不用高密度列表行高。 */
    const cellPad =
        mode === "edit"
            ? "h-auto min-h-11 px-3 py-3"
            : "h-auto min-h-(--table-row-height) px-3 py-2.5"

    return (
        <section
            data-slot="editable-line-item-table"
            data-mode={mode}
            className={cn("space-y-4", className)}
            {...props}
        >
            <div className="overflow-hidden rounded-2xl border border-border bg-card">
                <Table data-density="comfortable">
                    <caption className="sr-only">{caption}</caption>
                    <TableHeader>
                        <TableRow className="hover:bg-transparent">
                            {columns.map((column) => (
                                <TableHead
                                    key={column.id}
                                    scope="col"
                                    className={cn(
                                        "h-auto min-h-10 px-3 py-2.5",
                                        alignmentClass[column.align ?? "start"],
                                        column.numeric && "num",
                                    )}
                                >
                                    {column.header}
                                </TableHead>
                            ))}
                            {hasRowActions ? (
                                <TableHead
                                    scope="col"
                                    className="h-auto min-h-10 px-3 py-2.5 text-right"
                                >
                                    操作
                                </TableHead>
                            ) : null}
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {items.length === 0 ? (
                            <TableRow>
                                <TableCell
                                    colSpan={Math.max(columnCount, 1)}
                                    className="h-auto whitespace-normal px-3 py-10 text-center text-muted-foreground"
                                >
                                    {emptyContent}
                                </TableCell>
                            </TableRow>
                        ) : (
                            items.map((item, rowIndex) => {
                                const rowId = getRowId(item, rowIndex)
                                const context: EditableLineItemContext<TItem> =
                                    {
                                        item,
                                        rowId,
                                        rowIndex,
                                        mode,
                                        disabled,
                                    }
                                const rowErrors = getRowErrors?.(context) ?? []
                                const removeDisabledReason =
                                    getRemoveDisabledReason?.(context)

                                return (
                                    <React.Fragment key={rowId}>
                                        <TableRow
                                            aria-invalid={
                                                rowErrors.length > 0 ||
                                                undefined
                                            }
                                        >
                                            {columns.map((column) => {
                                                const content =
                                                    mode === "edit" &&
                                                    column.renderEditor
                                                        ? column.renderEditor(
                                                              context,
                                                          )
                                                        : column.renderValue(
                                                              context,
                                                          )

                                                return (
                                                    <TableCell
                                                        key={column.id}
                                                        className={cn(
                                                            "whitespace-normal align-middle",
                                                            cellPad,
                                                            alignmentClass[
                                                                column.align ??
                                                                    "start"
                                                            ],
                                                            column.numeric &&
                                                                "num",
                                                        )}
                                                    >
                                                        {content}
                                                    </TableCell>
                                                )
                                            })}
                                            {hasRowActions ? (
                                                <TableCell
                                                    className={cn(
                                                        "align-middle",
                                                        cellPad,
                                                    )}
                                                >
                                                    <div className="flex justify-end gap-1.5">
                                                        {renderRowActions?.(
                                                            context,
                                                        )}
                                                        {onRemoveItem ? (
                                                            <GuardedBusinessAction
                                                                type="button"
                                                                variant="ghost"
                                                                size="icon-sm"
                                                                aria-label={`删除第 ${rowIndex + 1} 行`}
                                                                disabled={
                                                                    disabled ||
                                                                    Boolean(
                                                                        removeDisabledReason,
                                                                    )
                                                                }
                                                                reason={
                                                                    removeDisabledReason ??
                                                                    (disabled
                                                                        ? "当前表单不可编辑"
                                                                        : undefined)
                                                                }
                                                                onClick={() =>
                                                                    onRemoveItem(
                                                                        item,
                                                                        rowId,
                                                                        rowIndex,
                                                                    )
                                                                }
                                                            >
                                                                <Trash2Icon aria-hidden="true" />
                                                            </GuardedBusinessAction>
                                                        ) : null}
                                                    </div>
                                                </TableCell>
                                            ) : null}
                                        </TableRow>
                                        {rowErrors.length > 0 ? (
                                            <TableRow data-slot="editable-line-item-errors">
                                                <TableCell
                                                    colSpan={Math.max(
                                                        columnCount,
                                                        1,
                                                    )}
                                                    className="h-auto whitespace-normal px-3 py-3"
                                                >
                                                    <Alert variant="destructive">
                                                        <TriangleAlertIcon aria-hidden="true" />
                                                        <AlertTitle>
                                                            第 {rowIndex + 1}{" "}
                                                            行存在错误
                                                        </AlertTitle>
                                                        <AlertDescription>
                                                            <ul className="list-disc space-y-1 pl-5">
                                                                {rowErrors.map(
                                                                    (
                                                                        error,
                                                                        errorIndex,
                                                                    ) => (
                                                                        <li
                                                                            key={
                                                                                errorIndex
                                                                            }
                                                                        >
                                                                            {
                                                                                error
                                                                            }
                                                                        </li>
                                                                    ),
                                                                )}
                                                            </ul>
                                                        </AlertDescription>
                                                    </Alert>
                                                </TableCell>
                                            </TableRow>
                                        ) : null}
                                    </React.Fragment>
                                )
                            })
                        )}
                    </TableBody>
                </Table>
            </div>

            {footer || onAddItem ? (
                <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                    <div className="min-w-0 flex-1">{footer}</div>
                    {onAddItem ? (
                        <GuardedBusinessAction
                            type="button"
                            variant="outline"
                            disabled={!canAdd}
                            reason={
                                addDisabledReason ??
                                (disabled ? "当前表单不可编辑" : undefined)
                            }
                            onClick={onAddItem}
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            {addLabel}
                        </GuardedBusinessAction>
                    ) : null}
                </div>
            ) : null}
        </section>
    )
}

interface ApprovalSummaryItem {
    readonly id: string
    readonly label: string
    readonly value: React.ReactNode
    readonly numeric?: boolean
}

type ApprovalPendingDecision = "approve" | "reject" | null

interface ApprovalDecisionPanelProps extends Omit<
    React.ComponentPropsWithoutRef<"section">,
    "children"
> {
    readonly title?: string
    readonly description?: React.ReactNode
    readonly summaryItems: readonly ApprovalSummaryItem[]
    /** 由页面注入的 TanStack Form 审批意见字段。 */
    readonly opinionField: React.ReactNode
    readonly effects?: readonly React.ReactNode[]
    readonly blockers?: readonly React.ReactNode[]
    readonly approveLabel?: string
    readonly rejectLabel?: string
    readonly approveDisabled?: boolean
    readonly rejectDisabled?: boolean
    readonly approveDisabledReason?: string
    readonly rejectDisabledReason?: string
    readonly pendingDecision?: ApprovalPendingDecision
    readonly onApprove: () => void
    readonly onReject: () => void
}

/** 审批摘要、意见字段及正式决定影响的受控组合面板。 */
function ApprovalDecisionPanel({
    title = "审批决定",
    description = "核对摘要与动作影响，填写审批意见后作出决定。",
    summaryItems,
    opinionField,
    effects = [],
    blockers = [],
    approveLabel = "同意",
    rejectLabel = "驳回",
    approveDisabled = false,
    rejectDisabled = false,
    approveDisabledReason,
    rejectDisabledReason,
    pendingDecision = null,
    onApprove,
    onReject,
    className,
    ...props
}: ApprovalDecisionPanelProps) {
    const isPending = pendingDecision != null

    return (
        <section
            data-slot="approval-decision-panel"
            className={className}
            {...props}
        >
            <Card>
                <CardHeader className="border-b border-border">
                    <CardTitle>{title}</CardTitle>
                    <CardDescription>{description}</CardDescription>
                </CardHeader>

                <CardContent className="space-y-5">
                    <DescriptionList columns="two" aria-label="审批摘要">
                        {summaryItems.map((item) => (
                            <DescriptionItem key={item.id}>
                                <DescriptionTerm>{item.label}</DescriptionTerm>
                                <DescriptionDetails
                                    className={cn(item.numeric && "num")}
                                >
                                    {item.value}
                                </DescriptionDetails>
                            </DescriptionItem>
                        ))}
                    </DescriptionList>

                    <div data-slot="approval-opinion-field">{opinionField}</div>

                    {effects.length > 0 ? (
                        <Alert variant="info">
                            <CircleCheckIcon aria-hidden="true" />
                            <AlertTitle>本次决定的影响</AlertTitle>
                            <AlertDescription>
                                <ul className="space-y-1">
                                    {effects.map((effect, index) => (
                                        <li
                                            key={index}
                                            className="flex items-start gap-2"
                                        >
                                            <CheckIcon
                                                aria-hidden="true"
                                                className="mt-0.5 size-4 shrink-0"
                                            />
                                            <span>{effect}</span>
                                        </li>
                                    ))}
                                </ul>
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {blockers.length > 0 ? (
                        <Alert variant="warning">
                            <TriangleAlertIcon aria-hidden="true" />
                            <AlertTitle>当前动作阻断</AlertTitle>
                            <AlertDescription>
                                <ul className="list-disc space-y-1 pl-5">
                                    {blockers.map((blocker, index) => (
                                        <li key={index}>{blocker}</li>
                                    ))}
                                </ul>
                            </AlertDescription>
                        </Alert>
                    ) : null}
                </CardContent>

                <CardFooter className="justify-end gap-2 border-t border-border">
                    <GuardedBusinessAction
                        type="button"
                        variant="outline"
                        disabled={isPending || rejectDisabled}
                        reason={
                            pendingDecision
                                ? sequentialText.decisionSubmitting
                                : rejectDisabledReason
                        }
                        onClick={onReject}
                    >
                        {pendingDecision === "reject" ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                                className="animate-spin"
                            />
                        ) : (
                            <CircleXIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        {rejectLabel}
                    </GuardedBusinessAction>
                    <GuardedBusinessAction
                        type="button"
                        disabled={isPending || approveDisabled}
                        reason={
                            pendingDecision
                                ? sequentialText.decisionSubmitting
                                : approveDisabledReason
                        }
                        onClick={onApprove}
                    >
                        {pendingDecision === "approve" ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                                className="animate-spin"
                            />
                        ) : (
                            <CircleCheckIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                        )}
                        {approveLabel}
                    </GuardedBusinessAction>
                </CardFooter>
            </Card>
        </section>
    )
}

interface AllocationSummary {
    readonly totalToAllocate: React.ReactNode
    readonly allocated: React.ReactNode
    readonly difference: React.ReactNode
}

interface AllocationWorkspaceProps<TAllocation> extends Omit<
    React.ComponentPropsWithoutRef<"section">,
    "children"
> {
    readonly title?: string
    readonly description?: React.ReactNode
    /** 所有数量均由调用方计算并传入；组件不推导差额。 */
    readonly summary: AllocationSummary
    readonly allocations: readonly TAllocation[]
    readonly columns: readonly EditableLineItemColumn<TAllocation>[]
    readonly getRowId: (allocation: TAllocation, rowIndex: number) => string
    readonly getRowErrors?: (
        context: EditableLineItemContext<TAllocation>,
    ) => readonly React.ReactNode[]
    readonly getRemoveDisabledReason?: (
        context: EditableLineItemContext<TAllocation>,
    ) => string | undefined
    readonly statusNotice?: React.ReactNode
    readonly actions?: React.ReactNode
    readonly disabled?: boolean
    readonly addLabel?: string
    readonly addDisabledReason?: string
    readonly onAddAllocation?: () => void
    readonly onRemoveAllocation?: (
        allocation: TAllocation,
        rowId: string,
        rowIndex: number,
    ) => void
}

/** 分配数量摘要与分配行字段的受控工作区，不在组件内计算任何业务数值。 */
function AllocationWorkspace<TAllocation>({
    title = "分配明细",
    description = "逐行填写分配对象与数量，并核对调用方提供的差额。",
    summary,
    allocations,
    columns,
    getRowId,
    getRowErrors,
    getRemoveDisabledReason,
    statusNotice,
    actions,
    disabled = false,
    addLabel = "新增分配行",
    addDisabledReason,
    onAddAllocation,
    onRemoveAllocation,
    className,
    ...props
}: AllocationWorkspaceProps<TAllocation>) {
    return (
        <section
            data-slot="allocation-workspace"
            className={className}
            {...props}
        >
            <Card>
                <CardHeader className="border-b border-border">
                    <CardTitle>{title}</CardTitle>
                    <CardDescription>{description}</CardDescription>
                </CardHeader>

                <CardContent className="space-y-5">
                    <DescriptionList columns="three" aria-label="分配数量摘要">
                        <DescriptionItem>
                            <DescriptionTerm>待分配总量</DescriptionTerm>
                            <DescriptionDetails className="num text-base font-medium">
                                {summary.totalToAllocate}
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>已分配</DescriptionTerm>
                            <DescriptionDetails className="num text-base font-medium">
                                {summary.allocated}
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>差额</DescriptionTerm>
                            <DescriptionDetails className="num text-base font-medium">
                                {summary.difference}
                            </DescriptionDetails>
                        </DescriptionItem>
                    </DescriptionList>

                    {statusNotice}

                    <EditableLineItemTable
                        items={allocations}
                        columns={columns}
                        getRowId={getRowId}
                        mode="edit"
                        disabled={disabled}
                        caption="分配明细"
                        emptyContent="暂无分配行，可新增一行后填写分配对象与数量。"
                        addLabel={addLabel}
                        addDisabledReason={addDisabledReason}
                        onAddItem={onAddAllocation}
                        onRemoveItem={onRemoveAllocation}
                        getRemoveDisabledReason={getRemoveDisabledReason}
                        getRowErrors={getRowErrors}
                    />
                </CardContent>

                {actions ? (
                    <CardFooter className="justify-end gap-2 border-t border-border">
                        {actions}
                    </CardFooter>
                ) : null}
            </Card>
        </section>
    )
}

export {
    AllocationWorkspace,
    ApprovalDecisionPanel,
    EditableLineItemTable,
    StickyTotalBar,
    type AllocationSummary,
    type AllocationWorkspaceProps,
    type ApprovalDecisionPanelProps,
    type ApprovalPendingDecision,
    type ApprovalSummaryItem,
    type CellAlignment,
    type EditableLineItemColumn,
    type EditableLineItemContext,
    type EditableLineItemTableProps,
    type LineItemMode,
    type StickyTotalBarProps,
    type StickyTotalItem,
}
