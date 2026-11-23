"use client"

import type { ListWorkspaceViewItem } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type {
    WorkspaceSearchOption,
    WorkspaceSearchOptions,
} from "@/lib/workspace-search"
import type { WorkspaceUrlState } from "../lib/url-state"
import { queueTypeLabel } from "../lib/queue-presentation"

/** 待执行桶涵盖的全部 13 类结构化业务执行事项（与后端 WORK_ITEM_TYPES 严格齐套）。 */
export const EXECUTION_WORK_ITEM_TYPES = [
    "SALES_INQUIRY_PROCESSING",
    "PROCUREMENT_ORDER_CREATION",
    "PURCHASE_SOURCE_UPDATE",
    "PURCHASE_PAYMENT_ARRANGEMENT",
    "SUPPLIER_PORTAL_REVIEW",
    "FULFILLMENT_OPERATION",
    "CUSTOMER_ACCEPTANCE_REGISTRATION",
    "SUPPLIER_PAYMENT_EXECUTION",
    "SALES_INVOICE_EXECUTION",
    "PURCHASE_RETURN_REFUND",
    "PURCHASE_RETURN_INVOICE",
    "SUPPLIER_SETTLEMENT_REVIEW",
    "IMPORT_BUSINESS_CONFIRMATION",
] as const

export const EXECUTION_ITEM_TYPES: readonly string[] =
    EXECUTION_WORK_ITEM_TYPES.map((type) => `task:${type}`)

/** 异常处理桶涵盖的 2 类异常治理事项（与后端 stats.exception 口径一致）。 */
export const EXCEPTION_WORK_ITEM_TYPES = [
    "INTEGRATION_RESULT_UNKNOWN",
    "BUSINESS_EXCEPTION",
] as const

export const EXCEPTION_ITEM_TYPES: readonly string[] =
    EXCEPTION_WORK_ITEM_TYPES.map((type) => `task:${type}`)

export type WorkspaceQueueBucketId =
    | "all"
    | "approval"
    | "execution"
    | "exception"
    | "rework"

export type WorkspaceQueueBucket = Readonly<{
    id: WorkspaceQueueBucketId
    label: string
    itemTypes: readonly string[]
}>

/** `/workspace/tasks` 顶栏 5 个清晰聚合桶：全部、待审批、待执行、异常处理、退回重办。 */
export const WORKSPACE_QUEUE_BUCKETS: readonly WorkspaceQueueBucket[] = [
    { id: "all", label: "全部", itemTypes: [] },
    { id: "approval", label: "待审批", itemTypes: ["task:DOCUMENT_APPROVAL"] },
    { id: "execution", label: "待执行", itemTypes: EXECUTION_ITEM_TYPES },
    { id: "exception", label: "异常处理", itemTypes: EXCEPTION_ITEM_TYPES },
    { id: "rework", label: "退回重办", itemTypes: ["task:APPROVAL_REWORK"] },
]

const EXECUTION_SET = new Set<string>(EXECUTION_ITEM_TYPES)
const EXCEPTION_SET = new Set<string>(EXCEPTION_ITEM_TYPES)

/** 由当前 `filters.item_types` 反推所属顶层聚合桶；跨桶自定义多选时返回 `null`。 */
export function resolveActiveWorkspaceBucket(
    selectedTypes: readonly string[] | undefined,
): WorkspaceQueueBucketId | null {
    if (!selectedTypes?.length) return "all"
    if (
        selectedTypes.every(
            (type) =>
                type === "task:DOCUMENT_APPROVAL" ||
                type.startsWith("approval:"),
        )
    ) {
        return "approval"
    }
    if (selectedTypes.every((type) => EXECUTION_SET.has(type))) {
        return "execution"
    }
    if (selectedTypes.every((type) => EXCEPTION_SET.has(type))) {
        return "exception"
    }
    if (
        selectedTypes.length === 1 &&
        selectedTypes[0] === "task:APPROVAL_REWORK"
    ) {
        return "rework"
    }
    return null
}

/** 判断当前选中的 `item_types` 是否恰好等于某个顶层聚合桶（用于避免重复芯片与区分二级细分）。 */
export function isExactWorkspaceBucketSelection(
    selectedTypes: readonly string[] | undefined,
    bucketId: WorkspaceQueueBucketId,
): boolean {
    const types = selectedTypes ?? []
    switch (bucketId) {
        case "all":
            return types.length === 0
        case "approval":
            return types.length === 1 && types[0] === "task:DOCUMENT_APPROVAL"
        case "execution":
            return (
                types.length === EXECUTION_ITEM_TYPES.length &&
                EXECUTION_ITEM_TYPES.every((item) => types.includes(item))
            )
        case "exception":
            return (
                types.length === EXCEPTION_ITEM_TYPES.length &&
                EXCEPTION_ITEM_TYPES.every((item) => types.includes(item))
            )
        case "rework":
            return types.length === 1 && types[0] === "task:APPROVAL_REWORK"
    }
}

/** 生成内联至 44px `ListWorkspacePage` 页头的 5 桶视图项。 */
export function buildWorkspaceBucketViews({
    selectedTypes,
    counts,
    onSelectItemTypes,
}: {
    selectedTypes: readonly string[] | undefined
    counts: Record<string, number | undefined>
    onSelectItemTypes: (itemTypes: string[]) => void
}): ListWorkspaceViewItem[] {
    const activeBucket = resolveActiveWorkspaceBucket(selectedTypes)
    return WORKSPACE_QUEUE_BUCKETS.map((bucket) => ({
        id: `workspace-bucket-${bucket.id}`,
        label: bucket.label,
        count: counts[`bucket:${bucket.id}`],
        active: activeBucket === bucket.id,
        onClick: () => onSelectItemTypes([...bucket.itemTypes]),
    }))
}

function belongsToBucket(
    value: string,
    bucket: WorkspaceQueueBucketId,
): boolean {
    switch (bucket) {
        case "approval":
            return value.startsWith("approval:")
        case "execution":
            return EXECUTION_SET.has(value)
        case "exception":
            return EXCEPTION_SET.has(value)
        default:
            return false
    }
}

/**
 * 二级类型细化条：仅在选中含多种细类的顶层桶（待审批 / 待执行 / 异常处理）且存在细分类型时按需出现，
 * 默认「全部」态不占任何纵向空间，消除 85px 双层页头。
 */
export function WorkspaceTypeNav({
    state,
    options,
    counts,
    onChange,
}: {
    state: WorkspaceUrlState
    options: WorkspaceSearchOptions
    counts: Record<string, number | undefined>
    onChange: (types: string[]) => void
}) {
    if (state.view === "started") return null
    const selected = state.filters?.item_types ?? []
    const activeBucket = resolveActiveWorkspaceBucket(selected)
    if (!activeBucket || activeBucket === "all" || activeBucket === "rework") {
        return null
    }
    const bucket = WORKSPACE_QUEUE_BUCKETS.find(
        (entry) => entry.id === activeBucket,
    )
    if (!bucket) return null

    const allOptions = options.item_types ?? []
    const subtypes: WorkspaceSearchOption[] = allOptions.filter(
        (option) =>
            belongsToBucket(option.value, activeBucket) &&
            (selected.includes(option.value) ||
                counts[option.value] === undefined ||
                (counts[option.value] ?? 0) > 0),
    )

    // 仅存在 0~1 个细类且未主动筛选单一细类时，不额外渲染二级条。
    const exactBucket = isExactWorkspaceBucketSelection(selected, activeBucket)
    if (subtypes.length <= 1 && exactBucket) return null

    return (
        <div
            data-slot="workspace-secondary-type-bar"
            role="group"
            aria-label={`${bucket.label}细分类`}
            className="flex shrink-0 items-center gap-1.5 overflow-x-auto border-b border-border/50 bg-card px-4 py-1.5"
        >
            <Button
                id={`workspace-type-${activeBucket}-all`}
                type="button"
                variant="ghost"
                size="xs"
                aria-pressed={exactBucket}
                className={cn(
                    "h-6 shrink-0 rounded-full px-2.5 text-xs font-normal text-muted-foreground shadow-none hover:bg-muted/80 hover:text-foreground",
                    exactBucket &&
                        "bg-sidebar-accent/80 font-medium text-sidebar-accent-foreground hover:bg-sidebar-accent",
                )}
                onClick={() => onChange([...bucket.itemTypes])}
            >
                全部{bucket.label}
                {counts[`bucket:${activeBucket}`] != null ? (
                    <span className="num text-muted-foreground/80">
                        {counts[`bucket:${activeBucket}`]}
                    </span>
                ) : null}
            </Button>
            {subtypes.map((subtype) => {
                const isSelected =
                    !exactBucket &&
                    selected.length === 1 &&
                    selected[0] === subtype.value
                const count = counts[subtype.value]
                return (
                    <Button
                        key={subtype.value}
                        id={`workspace-type-${toAutomationIdSegment(subtype.value)}`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        aria-pressed={isSelected}
                        className={cn(
                            "h-6 shrink-0 rounded-full px-2.5 text-xs font-normal text-muted-foreground shadow-none hover:bg-muted/80 hover:text-foreground",
                            isSelected &&
                                "bg-sidebar-accent/80 font-medium text-sidebar-accent-foreground hover:bg-sidebar-accent",
                        )}
                        onClick={() =>
                            onChange(
                                isSelected
                                    ? [...bucket.itemTypes]
                                    : [subtype.value],
                            )
                        }
                    >
                        {queueTypeLabel(subtype)}
                        {count != null ? (
                            <span className="num text-muted-foreground/80">
                                {count}
                            </span>
                        ) : null}
                    </Button>
                )
            })}
        </div>
    )
}
