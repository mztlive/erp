"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"
import {
    ArrowLeftIcon,
    ArrowRightIcon,
    RefreshCwIcon,
    SearchIcon,
    ShieldAlertIcon,
} from "lucide-react"
import { z } from "zod"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    DataFreshness,
    FixedOptionCheckboxFilter,
    OptionCombobox,
    PageHeader,
    PageScaffold,
    SequentialProcessBar,
    surfacePanelClassName,
    WorkTaskItem,
    type ResponsibilityStatus,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
    useBlockedApprovalsQuery,
    useRecoverApprovalMutation,
    useWorkItemResponsibilityMutation,
    type BlockedApprovalView,
    type WorkItemAllowedAction,
} from "@/features/work-items"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useTeamOptionsQuery } from "@/hooks/use-options"
import { hasAnyPermission } from "@/lib/permissions"
import { responsibilityText } from "@/lib/ui-text"
import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
} from "@/lib/formal-command"
import { cn } from "@/lib/utils"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Separator } from "@/components/ui/separator"

import { buildFilterSummary } from "../lib/filter-work-items"
import { buildHandlerHref } from "../lib/handler-destination"
import {
    buildW02SearchParams,
    parseDue,
    parseFamily,
    parsePriorities,
    parseScopeSlug,
    parseSort,
    readW02FocusId,
    scopeLabel,
    writeW02FocusId,
} from "../lib/queue-url"
import { useUnifiedTaskQueueQuery } from "../hooks/queries"
import { FAMILY_LABELS, type QueueWorkItemView } from "../types"

const reasonSchema = z
    .object({
        reason: z
            .string()
            .trim()
            .min(1, "请填写原因")
            .max(500, "原因不超过 500 字"),
        targetUserId: z.string(),
        reasonCode: z.enum(["DUPLICATE", "MISROUTED"]),
        replacementWorkItemId: z.string(),
    })
    .superRefine((value, context) => {
        if (
            value.reasonCode === "DUPLICATE" &&
            !value.replacementWorkItemId.trim()
        ) {
            context.addIssue({
                code: "custom",
                path: ["replacementWorkItemId"],
                message: "重复任务必须选择有效替代任务",
            })
        }
    })

const IDEMPOTENCY_PREFIX = "work-item-responsibility"

function createIdempotencyKey(workItemId: string, action: string): string {
    return `${IDEMPOTENCY_PREFIX}:${workItemId}:${action}:${crypto.randomUUID()}`
}

function toResponsibilityStatus(
    item: QueueWorkItemView,
    scope: "mine" | "team" | "managed" | "history",
    currentUserId?: string,
): ResponsibilityStatus {
    if (item.status === "COMPLETED") return "completed"
    if (item.status === "CLOSED") return "closed"
    if (item.processingState === "APPROVAL_BLOCKED") return "blocked"
    if (item.assignmentMode === "POOL" && item.ownerUser === undefined) {
        return "pool_available"
    }
    return scope === "mine" || item.ownerUser?.id === currentUserId
        ? "assigned_to_me"
        : "assigned_to_other"
}

function containsAction(
    item: QueueWorkItemView,
    action: WorkItemAllowedAction,
): boolean {
    return item.allowedActions.includes(action)
}

function BlockedApprovalList({ canRecover }: { canRecover: boolean }) {
    const query = useBlockedApprovalsQuery(canRecover)
    const recover = useRecoverApprovalMutation()
    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onChange: z.object({
                reason: z
                    .string()
                    .trim()
                    .min(1, "请填写恢复原因")
                    .max(500, "原因不超过 500 字"),
            }),
        },
        onSubmit: async () => undefined,
    })
    const [selected, setSelected] = React.useState<
        BlockedApprovalView | undefined
    >()
    const idempotencyKeys = React.useRef(new Map<string, string>())

    if (!canRecover) {
        return (
            <BusinessFailureState
                kind="permission"
                title="无权恢复受阻审批"
                description="请联系具备审批恢复权限的系统管理员。"
            />
        )
    }

    if (query.isPending) {
        return <div className="h-56 animate-pulse rounded-lg bg-muted" />
    }
    if (query.isError) {
        return (
            <BusinessFailureState
                error={query.error}
                onRetry={() => void query.refetch()}
            />
        )
    }
    if (!query.data?.items.length) {
        return (
            <BusinessEmptyState
                kind="no-exceptions"
                title="当前没有受阻审批"
                description="需要管理员重试的审批会显示在这里。"
            />
        )
    }

    return (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(18rem,1fr)]">
            <div className="space-y-2">
                {query.data.items.map((approval) => (
                    <button
                        key={approval.approvalInstanceId}
                        type="button"
                        className={cn(
                            "w-full rounded-lg border p-4 text-left",
                            selected?.approvalInstanceId ===
                                approval.approvalInstanceId
                                ? "border-primary bg-primary/5"
                                : "border-border bg-card",
                        )}
                        onClick={() => {
                            setSelected(approval)
                            form.reset()
                        }}
                    >
                        <p className="font-medium">
                            {approval.businessObjectLabel}
                        </p>
                        <p className="mt-1 text-sm text-muted-foreground">
                            {approval.blockerMessage}
                        </p>
                    </button>
                ))}
            </div>
            <Card>
                <CardHeader>
                    <CardTitle>重试当前步骤</CardTitle>
                    <CardDescription>
                        只重新执行当前步骤，不指定处理人，也不跳过审批。
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    {selected ? (
                        <form
                            className="space-y-4"
                            onSubmit={(event) => {
                                event.preventDefault()
                                void form.handleSubmit()
                            }}
                        >
                            <form.AppField
                                name="reason"
                                children={(field) => (
                                    <field.TextareaField
                                        label="重试原因"
                                        rows={4}
                                    />
                                )}
                            />
                            {recover.isError ? (
                                <BusinessFailureState
                                    error={recover.error}
                                    title="当前步骤尚未恢复"
                                />
                            ) : null}
                            <form.Subscribe
                                selector={(state) =>
                                    [
                                        state.canSubmit,
                                        state.values.reason,
                                    ] as const
                                }
                            >
                                {([canSubmit, reason]) => (
                                    <Button
                                        type="button"
                                        disabled={
                                            !canSubmit ||
                                            recover.isPending ||
                                            !selected.allowedActions.includes(
                                                "RETRY_CURRENT_STEP",
                                            )
                                        }
                                        onClick={() => {
                                            const workItem = selected.workItem
                                            const key =
                                                idempotencyKeys.current.get(
                                                    selected.approvalInstanceId,
                                                ) ??
                                                createIdempotencyKey(
                                                    selected.approvalInstanceId,
                                                    "recover",
                                                )
                                            idempotencyKeys.current.set(
                                                selected.approvalInstanceId,
                                                key,
                                            )
                                            void recover
                                                .mutateAsync({
                                                    approvalInstanceId:
                                                        selected.approvalInstanceId,
                                                    currentStepInstanceId:
                                                        selected.currentStepInstanceId,
                                                    expectedInstanceVersion:
                                                        selected.instanceVersion,
                                                    expectedStepVersion:
                                                        selected.stepVersion,
                                                    expectedTaskVersion:
                                                        workItem?.taskVersion,
                                                    recoveryAction:
                                                        "RETRY_CURRENT_STEP",
                                                    reason,
                                                    idempotencyKey: key,
                                                })
                                                .then(() => {
                                                    idempotencyKeys.current.delete(
                                                        selected.approvalInstanceId,
                                                    )
                                                })
                                                .catch(() => undefined)
                                        }}
                                    >
                                        {recover.isPending
                                            ? "正在重试"
                                            : "重试当前步骤"}
                                    </Button>
                                )}
                            </form.Subscribe>
                        </form>
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            请选择一项受阻审批。
                        </p>
                    )}
                </CardContent>
            </Card>
        </div>
    )
}

export function UnifiedTaskQueuePage() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const approvalBlockers = searchParams.get("view") === "approval-blockers"
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.data?.permissions
    const canManage = hasAnyPermission(permissions, [
        "work_item:reassign",
        "work_item:manage",
    ])
    const canRecover = hasAnyPermission(permissions, [
        "approval_instance:recover",
    ])
    const scope = parseScopeSlug(searchParams.get("scope"))
    const family = parseFamily(searchParams.get("family"))
    const due = parseDue(searchParams.get("due"))
    const priorities = parsePriorities(searchParams.get("priority"))
    const sort = parseSort(searchParams.get("sort"))
    const historyStatus =
        scope === "history" && searchParams.get("status") === "closed"
            ? "CLOSED"
            : scope === "history"
              ? "COMPLETED"
              : undefined
    const workItemType = searchParams.get("type") ?? undefined
    const queryText = searchParams.get("q") ?? ""
    const queueContextId = searchParams.get("queueContextId") ?? undefined
    const currentWorkItemId =
        searchParams.get("currentWorkItemId") ?? readW02FocusId() ?? undefined
    const [searchDraft, setSearchDraft] = React.useState(queryText)

    const queueQuery = useUnifiedTaskQueueQuery({
        scope,
        family,
        workItemType,
        historyStatus,
        due,
        priorities,
        query: queryText || undefined,
        sort,
        queueContextId,
        currentWorkItemId,
        viewerKey: profileQuery.data
            ? `${profileQuery.data.userid}:${[...profileQuery.data.role_ids].sort().join(",")}`
            : "profile-pending",
    })
    const responsibility = useWorkItemResponsibilityMutation()
    const items = queueQuery.data?.items ?? []
    const selectedIndex = currentWorkItemId
        ? Math.max(
              0,
              items.findIndex((item) => item.workItemId === currentWorkItemId),
          )
        : 0
    const selected = items[selectedIndex]
    const canSelectReassignmentTarget = Boolean(
        selected?.allowedActions.includes("REASSIGN"),
    )
    const teamOptions = useTeamOptionsQuery(canSelectReassignmentTarget)
    const [action, setAction] = React.useState<
        "RELEASE_TO_TEAM" | "REASSIGN" | "CLOSE" | null
    >(null)
    const commandLedger = React.useRef(new FormalCommandKeyLedger())
    const actionForm = useAppForm({
        defaultValues: {
            reason: "",
            targetUserId: "",
            reasonCode: "MISROUTED" as z.input<
                typeof reasonSchema
            >["reasonCode"],
            replacementWorkItemId: "",
        },
        validators: { onChange: reasonSchema },
        onSubmit: async () => undefined,
    })

    const replaceUrl = React.useCallback(
        (overrides: {
            scope?: typeof scope
            family?: typeof family | null
            due?: typeof due | null
            priorities?: readonly number[] | null
            sort?: typeof sort
            historyStatus?: typeof historyStatus
            query?: string | null
            currentWorkItemId?: string | null
            approvalBlockers?: boolean
        }) => {
            const nextId =
                overrides.currentWorkItemId === undefined
                    ? currentWorkItemId
                    : overrides.currentWorkItemId
            writeW02FocusId(nextId ?? null)
            router.replace(
                `/workspace/tasks${buildW02SearchParams({
                    scope: overrides.scope ?? scope,
                    family:
                        overrides.family === null
                            ? null
                            : (overrides.family ?? family),
                    workItemType,
                    due: overrides.due === null ? null : (overrides.due ?? due),
                    priorities:
                        overrides.priorities === null
                            ? null
                            : (overrides.priorities ?? priorities),
                    sort: overrides.sort ?? sort,
                    historyStatus: overrides.historyStatus ?? historyStatus,
                    q:
                        overrides.query === null
                            ? null
                            : (overrides.query ?? queryText),
                    queueContextId,
                    currentWorkItemId: nextId,
                    approvalBlockers: overrides.approvalBlockers,
                })}`,
                { scroll: false },
            )
        },
        [
            currentWorkItemId,
            due,
            family,
            historyStatus,
            priorities,
            queryText,
            queueContextId,
            router,
            scope,
            sort,
            workItemType,
        ],
    )

    const runResponsibilityAction = React.useCallback(
        async (
            kind: "START_PROCESSING" | "RELEASE_TO_TEAM" | "REASSIGN" | "CLOSE",
            reason = "",
            targetUserId = "",
            reasonCode = "MISROUTED",
            replacementWorkItemId = "",
        ) => {
            if (!selected) return
            const slot = `${selected.workItemId}:${kind}`
            const base = {
                workItemId: selected.workItemId,
                expectedTaskVersion: selected.taskVersion,
            }
            try {
                if (kind === "START_PROCESSING") {
                    const command = commandLedger.current.acquire(
                        slot,
                        `${IDEMPOTENCY_PREFIX}:${selected.workItemId}:${kind}`,
                        { ...base, kind } as const,
                    )
                    await responsibility.mutateAsync({
                        ...command.payload,
                        idempotencyKey: command.idempotencyKey,
                    })
                } else if (kind === "RELEASE_TO_TEAM") {
                    const command = commandLedger.current.acquire(
                        slot,
                        `${IDEMPOTENCY_PREFIX}:${selected.workItemId}:${kind}`,
                        { ...base, kind, reason } as const,
                    )
                    await responsibility.mutateAsync({
                        ...command.payload,
                        idempotencyKey: command.idempotencyKey,
                    })
                } else if (kind === "REASSIGN") {
                    const command = commandLedger.current.acquire(
                        slot,
                        `${IDEMPOTENCY_PREFIX}:${selected.workItemId}:${kind}`,
                        { ...base, kind, targetUserId, reason } as const,
                    )
                    await responsibility.mutateAsync({
                        ...command.payload,
                        idempotencyKey: command.idempotencyKey,
                    })
                } else {
                    const command = commandLedger.current.acquire(
                        slot,
                        `${IDEMPOTENCY_PREFIX}:${selected.workItemId}:${kind}`,
                        {
                            ...base,
                            kind,
                            reasonCode,
                            replacementWorkItemId:
                                reasonCode === "DUPLICATE"
                                    ? replacementWorkItemId
                                    : undefined,
                            comment: reason,
                        } as const,
                    )
                    await responsibility.mutateAsync({
                        ...command.payload,
                        idempotencyKey: command.idempotencyKey,
                    })
                }
            } catch (error) {
                commandLedger.current.settle(
                    slot,
                    classifyFormalCommandError(error),
                )
                return
            }
            commandLedger.current.settle(slot, "succeeded")
            setAction(null)
            actionForm.reset()
        },
        [actionForm, responsibility, selected],
    )

    const handlerHref = selected ? buildHandlerHref(selected) : null
    const readonly = scope === "history"
    const responsibilityStatus = selected
        ? toResponsibilityStatus(selected, scope, profileQuery.data?.userid)
        : "blocked"

    return (
        <PageScaffold>
            <PageHeader
                title={approvalBlockers ? "受阻审批" : "统一待办"}
                description={
                    approvalBlockers
                        ? "重试当前受阻步骤，不改变处理人和审批路径。"
                        : "按当前责任处理任务；业务结论在对应页面提交。"
                }
                metadata={
                    approvalBlockers ? null : (
                        <DataFreshness
                            updatedAt="当前查询"
                            state={queueQuery.isFetching ? "syncing" : "fresh"}
                        />
                    )
                }
                actions={
                    <Button
                        type="button"
                        variant="outline"
                        onClick={() => {
                            if (approvalBlockers) {
                                replaceUrl({ approvalBlockers: false })
                            } else {
                                void queueQuery.refetch()
                            }
                        }}
                    >
                        <RefreshCwIcon aria-hidden="true" />
                        {approvalBlockers ? "返回待办" : "刷新"}
                    </Button>
                }
            />

            {approvalBlockers ? (
                <BlockedApprovalList canRecover={canRecover} />
            ) : (
                <>
                    <section
                        className={cn(
                            surfacePanelClassName,
                            "sticky top-0 z-10 space-y-3 p-3",
                        )}
                        aria-label="待办筛选"
                    >
                        <div className="flex flex-wrap gap-2">
                            {(["mine", "team", "history"] as const).map(
                                (value) => (
                                    <Button
                                        key={value}
                                        type="button"
                                        variant={
                                            scope === value
                                                ? "secondary"
                                                : "ghost"
                                        }
                                        onClick={() =>
                                            replaceUrl({
                                                scope: value,
                                                currentWorkItemId: null,
                                            })
                                        }
                                    >
                                        {scopeLabel(value)}
                                    </Button>
                                ),
                            )}
                            {canManage ? (
                                <Button
                                    type="button"
                                    variant={
                                        scope === "managed"
                                            ? "secondary"
                                            : "ghost"
                                    }
                                    onClick={() =>
                                        replaceUrl({
                                            scope: "managed",
                                            currentWorkItemId: null,
                                        })
                                    }
                                >
                                    团队任务
                                </Button>
                            ) : null}
                            {canRecover ? (
                                <Button
                                    type="button"
                                    variant="ghost"
                                    onClick={() =>
                                        replaceUrl({ approvalBlockers: true })
                                    }
                                >
                                    受阻审批
                                </Button>
                            ) : null}
                        </div>
                        <div className="grid gap-2 md:grid-cols-[minmax(14rem,1fr)_12rem_12rem_12rem_auto]">
                            <InputGroup>
                                <InputGroupAddon>
                                    <SearchIcon aria-hidden="true" />
                                </InputGroupAddon>
                                <InputGroupInput
                                    aria-label="搜索待办"
                                    value={searchDraft}
                                    placeholder="搜索单号、对象或往来方"
                                    onChange={(event) =>
                                        setSearchDraft(event.target.value)
                                    }
                                    onKeyDown={(event) => {
                                        if (event.key === "Enter") {
                                            replaceUrl({
                                                query: searchDraft,
                                                currentWorkItemId: null,
                                            })
                                        }
                                    }}
                                />
                            </InputGroup>
                            <OptionCombobox
                                aria-label="任务分类"
                                options={Object.entries(FAMILY_LABELS).map(
                                    ([value, label]) => ({ value, label }),
                                )}
                                value={family}
                                placeholder="全部分类"
                                onValueChange={(value) =>
                                    replaceUrl({
                                        family:
                                            (value as typeof family) ?? null,
                                        currentWorkItemId: null,
                                    })
                                }
                            />
                            {scope === "history" ? (
                                <OptionCombobox
                                    aria-label="历史结果"
                                    options={[
                                        {
                                            value: "COMPLETED",
                                            label: "已完成",
                                        },
                                        { value: "CLOSED", label: "已关闭" },
                                    ]}
                                    value={historyStatus}
                                    allowClear={false}
                                    onValueChange={(value) =>
                                        replaceUrl({
                                            historyStatus:
                                                value === "CLOSED"
                                                    ? "CLOSED"
                                                    : "COMPLETED",
                                            currentWorkItemId: null,
                                        })
                                    }
                                />
                            ) : null}
                            <OptionCombobox
                                aria-label="排序"
                                options={[
                                    {
                                        value: "priority_due",
                                        label: "优先级与时限",
                                    },
                                    {
                                        value: "due_asc",
                                        label: "截止时间",
                                    },
                                    {
                                        value: "created_desc",
                                        label: "最新进入",
                                    },
                                ]}
                                value={sort}
                                allowClear={false}
                                onValueChange={(value) =>
                                    replaceUrl({
                                        sort:
                                            value === "due_asc" ||
                                            value === "created_desc"
                                                ? value
                                                : "priority_due",
                                        currentWorkItemId: null,
                                    })
                                }
                            />
                            <OptionCombobox
                                aria-label="时限"
                                options={[
                                    { value: "overdue", label: "已超期" },
                                    { value: "today", label: "今日到期" },
                                ]}
                                value={due}
                                placeholder="全部时限"
                                onValueChange={(value) =>
                                    replaceUrl({
                                        due: (value as typeof due) ?? null,
                                        currentWorkItemId: null,
                                    })
                                }
                            />
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() => {
                                    setSearchDraft("")
                                    replaceUrl({
                                        family: null,
                                        due: null,
                                        priorities: null,
                                        query: null,
                                        currentWorkItemId: null,
                                    })
                                }}
                            >
                                清除筛选
                            </Button>
                        </div>
                        <FixedOptionCheckboxFilter
                            label="优先级"
                            value={(priorities ?? []).map(String)}
                            options={[
                                { value: "1", label: "紧急" },
                                { value: "2", label: "高" },
                                { value: "3", label: "普通" },
                                { value: "4", label: "低" },
                            ]}
                            onValueChange={(values) =>
                                replaceUrl({
                                    priorities: values.map(Number),
                                    currentWorkItemId: null,
                                })
                            }
                        />
                        <p className="text-xs text-muted-foreground">
                            {buildFilterSummary(
                                {
                                    scope,
                                    family,
                                    workItemType,
                                    historyStatus,
                                    due,
                                    priorities,
                                    query: queryText || undefined,
                                    sort,
                                },
                                queueQuery.data?.total ?? 0,
                            )}
                        </p>
                    </section>

                    {queueQuery.isPending ? (
                        <div className="h-96 animate-pulse rounded-lg bg-muted" />
                    ) : queueQuery.isError ? (
                        <BusinessFailureState
                            error={queueQuery.error}
                            onRetry={() => void queueQuery.refetch()}
                        />
                    ) : items.length === 0 ? (
                        <BusinessEmptyState
                            kind="no-tasks"
                            title="当前范围没有待办"
                            description="可调整责任范围或清除筛选后继续查看。"
                        />
                    ) : selected ? (
                        <div className="grid min-h-[32rem] gap-4 lg:grid-cols-[minmax(18rem,34%)_minmax(0,66%)]">
                            <section
                                className={cn(
                                    surfacePanelClassName,
                                    "max-h-[calc(100vh-18rem)] space-y-2 overflow-auto p-3",
                                )}
                                aria-label="任务队列"
                            >
                                {items.map((item) => (
                                    <button
                                        key={item.workItemId}
                                        type="button"
                                        className="block w-full text-left"
                                        aria-current={
                                            item.workItemId ===
                                                selected.workItemId || undefined
                                        }
                                        onClick={() =>
                                            replaceUrl({
                                                currentWorkItemId:
                                                    item.workItemId,
                                            })
                                        }
                                    >
                                        <WorkTaskItem
                                            density="compact"
                                            taskType={item.workItemTypeLabel}
                                            businessObject={item.businessObject}
                                            counterparty={item.counterparty}
                                            enteredAt={item.enteredAt}
                                            enteredDateTime={
                                                item.enteredDateTime
                                            }
                                            dueAt={item.dueLabel}
                                            dueDateTime={item.dueDateTime}
                                            responsibleParty={
                                                item.responsibilityLabel
                                            }
                                            reason={item.reason}
                                            impact={item.impact}
                                            status={item.statusPresentation}
                                            className={cn(
                                                item.workItemId ===
                                                    selected.workItemId &&
                                                    "border-primary bg-primary/5",
                                            )}
                                        />
                                    </button>
                                ))}
                            </section>

                            <section
                                className="space-y-4"
                                aria-label="当前任务"
                            >
                                <SequentialProcessBar
                                    current={selectedIndex + 1}
                                    total={items.length}
                                    responsibilityStatus={responsibilityStatus}
                                    responsibilityStatusLabel={
                                        selected.responsibilityLabel
                                    }
                                    processLabel="打开业务对象"
                                    showProcessNext={false}
                                    showProcess={!readonly}
                                    processDisabled={!handlerHref}
                                    pending={responsibility.isPending}
                                    onBack={() => router.back()}
                                    onProcess={() => {
                                        if (handlerHref)
                                            router.push(handlerHref)
                                    }}
                                    onProcessNext={() => undefined}
                                    onStartProcessing={
                                        containsAction(
                                            selected,
                                            "START_PROCESSING",
                                        )
                                            ? () => {
                                                  void runResponsibilityAction(
                                                      "START_PROCESSING",
                                                  )
                                              }
                                            : undefined
                                    }
                                />

                                <Card>
                                    <CardHeader>
                                        <div className="flex flex-wrap items-start justify-between gap-3">
                                            <div>
                                                <CardTitle>
                                                    {selected.businessObject}
                                                </CardTitle>
                                                <CardDescription>
                                                    {selected.workItemTypeLabel}
                                                </CardDescription>
                                            </div>
                                            <BusinessStatusBadge
                                                label={
                                                    selected.statusPresentation
                                                        .label
                                                }
                                                tone={
                                                    selected.statusPresentation
                                                        .tone
                                                }
                                            />
                                        </div>
                                    </CardHeader>
                                    <CardContent className="space-y-4">
                                        <dl className="grid gap-3 sm:grid-cols-2">
                                            <div>
                                                <dt className="text-xs text-muted-foreground">
                                                    为什么需要处理
                                                </dt>
                                                <dd className="mt-1 text-sm">
                                                    {selected.reason}
                                                </dd>
                                            </div>
                                            <div>
                                                <dt className="text-xs text-muted-foreground">
                                                    业务影响
                                                </dt>
                                                <dd className="mt-1 text-sm">
                                                    {selected.impact}
                                                </dd>
                                            </div>
                                            <div>
                                                <dt className="text-xs text-muted-foreground">
                                                    责任角色
                                                </dt>
                                                <dd className="mt-1 text-sm">
                                                    {selected.ownerRoleLabel}
                                                </dd>
                                            </div>
                                            <div>
                                                <dt className="text-xs text-muted-foreground">
                                                    当前处理人
                                                </dt>
                                                <dd className="mt-1 text-sm">
                                                    {
                                                        selected.responsibilityLabel
                                                    }
                                                </dd>
                                            </div>
                                        </dl>

                                        {!selected.handlerKnown ||
                                        !handlerHref ? (
                                            <Alert variant="warning">
                                                <ShieldAlertIcon aria-hidden="true" />
                                                <AlertTitle>
                                                    当前任务暂不可处理
                                                </AlertTitle>
                                                <AlertDescription>
                                                    {selected
                                                        .actionBlockers[0] ??
                                                        "任务处理入口尚未配置，请联系系统管理员。"}
                                                </AlertDescription>
                                            </Alert>
                                        ) : null}

                                        {responsibility.isError ? (
                                            <BusinessFailureState
                                                error={responsibility.error}
                                                title={
                                                    responsibilityText.changed
                                                }
                                                onRetry={() =>
                                                    void queueQuery.refetch()
                                                }
                                            />
                                        ) : null}

                                        {!readonly &&
                                        selected.status === "OPEN" ? (
                                            <>
                                                <Separator />
                                                <div className="flex flex-wrap gap-2">
                                                    {containsAction(
                                                        selected,
                                                        "RELEASE_TO_TEAM",
                                                    ) ? (
                                                        <Button
                                                            type="button"
                                                            variant="outline"
                                                            onClick={() =>
                                                                setAction(
                                                                    "RELEASE_TO_TEAM",
                                                                )
                                                            }
                                                        >
                                                            {
                                                                responsibilityText.releaseToTeam
                                                            }
                                                        </Button>
                                                    ) : null}
                                                    {containsAction(
                                                        selected,
                                                        "REASSIGN",
                                                    ) ? (
                                                        <Button
                                                            type="button"
                                                            variant="outline"
                                                            onClick={() =>
                                                                setAction(
                                                                    "REASSIGN",
                                                                )
                                                            }
                                                        >
                                                            {
                                                                responsibilityText.reassign
                                                            }
                                                        </Button>
                                                    ) : null}
                                                    {containsAction(
                                                        selected,
                                                        "CLOSE",
                                                    ) ? (
                                                        <Button
                                                            type="button"
                                                            variant="destructive"
                                                            onClick={() =>
                                                                setAction(
                                                                    "CLOSE",
                                                                )
                                                            }
                                                        >
                                                            关闭无效任务
                                                        </Button>
                                                    ) : null}
                                                </div>
                                            </>
                                        ) : null}
                                    </CardContent>
                                </Card>

                                {action ? (
                                    <Card>
                                        <CardHeader>
                                            <CardTitle>
                                                {action === "RELEASE_TO_TEAM"
                                                    ? "退回团队"
                                                    : action === "REASSIGN"
                                                      ? "转交任务"
                                                      : "关闭无效任务"}
                                            </CardTitle>
                                            <CardDescription>
                                                此操作只更新当前任务责任，不形成业务结论。
                                            </CardDescription>
                                        </CardHeader>
                                        <CardContent>
                                            <form
                                                className="space-y-4"
                                                onSubmit={(event) => {
                                                    event.preventDefault()
                                                    void actionForm.handleSubmit()
                                                }}
                                            >
                                                {action === "REASSIGN" ? (
                                                    <actionForm.AppField
                                                        name="targetUserId"
                                                        children={(field) => (
                                                            <field.SelectField
                                                                label="转交给"
                                                                options={(
                                                                    teamOptions.data ??
                                                                    []
                                                                ).map(
                                                                    (user) => ({
                                                                        value: user.userId,
                                                                        label: user.displayName,
                                                                    }),
                                                                )}
                                                            />
                                                        )}
                                                    />
                                                ) : null}
                                                {action === "CLOSE" ? (
                                                    <actionForm.AppField
                                                        name="reasonCode"
                                                        children={(field) => (
                                                            <field.SelectField
                                                                label="关闭原因"
                                                                options={[
                                                                    {
                                                                        value: "DUPLICATE",
                                                                        label: "重复任务",
                                                                    },
                                                                    {
                                                                        value: "MISROUTED",
                                                                        label: "任务误派",
                                                                    },
                                                                ]}
                                                            />
                                                        )}
                                                    />
                                                ) : null}
                                                {action === "CLOSE" ? (
                                                    <actionForm.Subscribe
                                                        selector={(state) =>
                                                            state.values
                                                                .reasonCode
                                                        }
                                                    >
                                                        {(reasonCode) =>
                                                            reasonCode ===
                                                            "DUPLICATE" ? (
                                                                <actionForm.AppField
                                                                    name="replacementWorkItemId"
                                                                    children={(
                                                                        field,
                                                                    ) => (
                                                                        <field.SelectField
                                                                            label="有效替代任务"
                                                                            options={items
                                                                                .filter(
                                                                                    (
                                                                                        item,
                                                                                    ) =>
                                                                                        item.workItemId !==
                                                                                            selected.workItemId &&
                                                                                        item.status ===
                                                                                            "OPEN" &&
                                                                                        item.workItemType ===
                                                                                            selected.workItemType &&
                                                                                        item.businessObjectType ===
                                                                                            selected.businessObjectType,
                                                                                )
                                                                                .map(
                                                                                    (
                                                                                        item,
                                                                                    ) => ({
                                                                                        value: item.workItemId,
                                                                                        label: item.businessObjectLabel,
                                                                                    }),
                                                                                )}
                                                                        />
                                                                    )}
                                                                />
                                                            ) : null
                                                        }
                                                    </actionForm.Subscribe>
                                                ) : null}
                                                <actionForm.AppField
                                                    name="reason"
                                                    children={(field) => (
                                                        <field.TextareaField
                                                            label="原因"
                                                            rows={3}
                                                        />
                                                    )}
                                                />
                                                <actionForm.Subscribe
                                                    selector={(state) =>
                                                        [
                                                            state.canSubmit,
                                                            state.values,
                                                        ] as const
                                                    }
                                                >
                                                    {([canSubmit, values]) => (
                                                        <div className="flex gap-2">
                                                            <Button
                                                                type="button"
                                                                variant="outline"
                                                                onClick={() => {
                                                                    setAction(
                                                                        null,
                                                                    )
                                                                    actionForm.reset()
                                                                }}
                                                            >
                                                                取消
                                                            </Button>
                                                            <Button
                                                                type="button"
                                                                disabled={
                                                                    !canSubmit ||
                                                                    responsibility.isPending ||
                                                                    (action ===
                                                                        "REASSIGN" &&
                                                                        !values.targetUserId)
                                                                }
                                                                onClick={() =>
                                                                    void runResponsibilityAction(
                                                                        action,
                                                                        values.reason,
                                                                        values.targetUserId,
                                                                        values.reasonCode,
                                                                        values.replacementWorkItemId,
                                                                    )
                                                                }
                                                            >
                                                                确认操作
                                                            </Button>
                                                        </div>
                                                    )}
                                                </actionForm.Subscribe>
                                            </form>
                                        </CardContent>
                                    </Card>
                                ) : null}

                                <div className="flex justify-between">
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        disabled={selectedIndex === 0}
                                        onClick={() =>
                                            replaceUrl({
                                                currentWorkItemId:
                                                    items[selectedIndex - 1]
                                                        ?.workItemId,
                                            })
                                        }
                                    >
                                        <ArrowLeftIcon aria-hidden="true" />
                                        上一项
                                    </Button>
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        disabled={
                                            selectedIndex >= items.length - 1
                                        }
                                        onClick={() =>
                                            replaceUrl({
                                                currentWorkItemId:
                                                    items[selectedIndex + 1]
                                                        ?.workItemId,
                                            })
                                        }
                                    >
                                        下一项
                                        <ArrowRightIcon aria-hidden="true" />
                                    </Button>
                                </div>
                            </section>
                        </div>
                    ) : null}
                </>
            )}
        </PageScaffold>
    )
}
