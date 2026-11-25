"use client"

import * as React from "react"
import { SearchIcon, UsersIcon, XIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
} from "@/components/business/feedback-states"
import {
    buildPersonPickerModel,
    filterPersonPickerModel,
    personMatchesQuery,
    type PersonPickerDepartment,
    type PersonPickerPerson,
} from "@/components/business/person-picker-model"
import { PersonPickerTree } from "@/components/business/person-picker-tree"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { RadioGroup } from "@/components/ui/radio-group"
import { Spinner } from "@/components/ui/spinner"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { listFilterText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

export type {
    PersonPickerDepartment,
    PersonPickerPerson,
} from "@/components/business/person-picker-model"

type PersonPickerCommonProps = {
    /** 同页各实例必须唯一，用于派生全部交互元素的 DOM id。 */
    id: string
    open: boolean
    onOpenChange: (open: boolean) => void
    /** 调用方传入完整且有权展示的部门和人员集合；组件不请求数据。 */
    departments: readonly PersonPickerDepartment[]
    people: readonly PersonPickerPerson[]
    loading?: boolean
    /** 传入可展示的失败文案；失败时保留草稿并禁止确认。 */
    error?: string | null
    onRetry?: () => void
    title?: string
    description?: string
    /** 调用方无法读取部门归属时，使用准确的分组文案，避免误报为未分配。 */
    unassignedDepartmentLabel?: string
    /** 默认必须选择至少一人；开启后允许确认空值以清除原选择。 */
    allowEmpty?: boolean
}

export type PersonPickerDialogProps = PersonPickerCommonProps &
    (
        | {
              mode: "single"
              value: string | null
              onConfirm: (value: string | null) => void
          }
        | {
              mode: "multiple"
              value: readonly string[]
              onConfirm: (value: string[]) => void
          }
    )

/** 只在确认时回填；关闭后丢弃草稿，下次打开从 value 重新初始化。 */
export function PersonPickerDialog(props: PersonPickerDialogProps) {
    return (
        <Dialog open={props.open} onOpenChange={props.onOpenChange}>
            {props.open && <PersonPickerSession key={props.mode} {...props} />}
        </Dialog>
    )
}

function PersonPickerSession(props: PersonPickerDialogProps) {
    const {
        id,
        departments,
        people,
        loading = false,
        error,
        onRetry,
        allowEmpty = false,
        unassignedDepartmentLabel = "未分配部门",
    } = props
    const multiple = props.mode === "multiple"
    const [draft, setDraft] = React.useState<string[]>(() =>
        props.mode === "multiple"
            ? [...new Set(props.value)]
            : props.value
              ? [props.value]
              : [],
    )
    const [query, setQuery] = React.useState("")
    const [collapsed, setCollapsed] = React.useState<ReadonlySet<string>>(
        new Set(),
    )
    const searchRef = React.useRef<HTMLInputElement>(null)
    const model = React.useMemo(
        () =>
            buildPersonPickerModel(
                departments,
                people,
                unassignedDepartmentLabel,
            ),
        [departments, people, unassignedDepartmentLabel],
    )
    const roots = React.useMemo(
        () => filterPersonPickerModel(model.roots, query),
        [model.roots, query],
    )
    const selected = new Set(draft)
    const busy = loading || Boolean(error)
    const invalidSelection = draft.some((personId) => {
        const person = model.byId.get(personId)
        return !person || person.disabled
    })
    const canConfirm =
        !busy && !invalidSelection && (allowEmpty || draft.length > 0)
    const unassigned = model.unassigned.filter(
        (person) =>
            unassignedDepartmentLabel.includes(query.trim()) ||
            personMatchesQuery(person, query),
    )
    const unknownDepartment = model.unknownDepartment.filter(
        (person) =>
            "部门信息不可用".includes(query.trim()) ||
            personMatchesQuery(person, query),
    )
    const hasResults =
        roots.length > 0 ||
        unassigned.length > 0 ||
        unknownDepartment.length > 0

    // 编码后再清洗，避免中文或大小写不同的业务键产生相同 DOM id。
    const itemId = (kind: string, key: string) =>
        `${id}-${kind}-${toAutomationIdSegment(Array.from(key, (char) => char.codePointAt(0)!.toString(16)).join("-"))}`
    const remove = (personId: string) =>
        setDraft((current) => current.filter((value) => value !== personId))
    const choose = (personId: string, checked: boolean) => {
        if (
            busy ||
            !model.byId.has(personId) ||
            model.byId.get(personId)?.disabled
        )
            return
        setDraft((current) =>
            multiple
                ? checked
                    ? [...new Set([...current, personId])]
                    : current.filter((value) => value !== personId)
                : [personId],
        )
    }

    const tree = (
        <PersonPickerTree
            roots={roots}
            unassigned={unassigned}
            unassignedDepartmentLabel={unassignedDepartmentLabel}
            unknownDepartment={unknownDepartment}
            selected={selected}
            collapsed={collapsed}
            setCollapsed={setCollapsed}
            query={query}
            multiple={multiple}
            itemId={itemId}
            choose={choose}
        />
    )

    const singlePerson = draft[0] ? model.byId.get(draft[0]) : undefined
    return (
        <DialogContent
            id={`${id}-dialog`}
            size="lg"
            scroll="body"
            closeButtonId={`${id}-close`}
            initialFocus={searchRef}
        >
            <DialogHeader>
                <DialogTitle>{props.title || "选择人员"}</DialogTitle>
                {props.description ? (
                    <DialogDescription>{props.description}</DialogDescription>
                ) : null}
            </DialogHeader>
            <div className="shrink-0 border-b border-border/60 px-overlay-padding py-3">
                <div className="relative">
                    <SearchIcon
                        aria-hidden="true"
                        className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                    />
                    <Input
                        ref={searchRef}
                        id={`${id}-search`}
                        value={query}
                        onChange={(event) => setQuery(event.target.value)}
                        placeholder="搜索姓名或部门"
                        aria-label="搜索姓名或部门"
                        className="pr-8 pl-8"
                    />
                    {query && (
                        <Button
                            id={`${id}-search-clear`}
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            className="absolute top-1/2 right-1 -translate-y-1/2"
                            aria-label="清除搜索"
                            onClick={() => {
                                setQuery("")
                                searchRef.current?.focus()
                            }}
                        >
                            <XIcon aria-hidden="true" />
                        </Button>
                    )}
                </div>
            </div>
            <DialogBody
                className={cn(
                    multiple &&
                        "grid gap-4 sm:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] sm:gap-0 sm:overflow-hidden sm:py-0",
                )}
            >
                <section
                    className={cn(
                        "flex min-h-0 flex-col",
                        multiple && "sm:py-4 sm:pr-5",
                    )}
                    aria-labelledby={`${id}-tree-title`}
                    aria-busy={loading}
                >
                    <h3
                        id={`${id}-tree-title`}
                        className="mb-2.5 font-semibold"
                    >
                        组织架构
                    </h3>
                    <div className="h-72 min-h-0 overflow-auto sm:h-[min(24rem,45dvh)]">
                        {error ? (
                            <BusinessFailureState
                                kind="system"
                                title="人员加载失败"
                                description={error}
                                action={
                                    onRetry ? (
                                        <Button
                                            id={`${id}-retry`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={loading}
                                            onClick={onRetry}
                                        >
                                            重新加载
                                        </Button>
                                    ) : undefined
                                }
                            />
                        ) : loading ? (
                            <div
                                role="status"
                                className="flex items-center justify-center gap-2 py-10 text-muted-foreground"
                            >
                                <Spinner aria-hidden="true" />
                                <span>正在加载人员…</span>
                            </div>
                        ) : !hasResults ? (
                            <BusinessEmptyState
                                kind={query.trim() ? "filter" : "no-data"}
                                title={
                                    query.trim()
                                        ? "没有符合条件的人员或部门"
                                        : "暂无可选择的人员"
                                }
                                description={
                                    query.trim()
                                        ? "没有符合当前关键词的人员或部门，请更换关键词或清除搜索。"
                                        : "当前范围内没有可选择的人员。"
                                }
                                className="py-8"
                            />
                        ) : multiple ? (
                            tree
                        ) : (
                            <RadioGroup
                                id={`${id}-people`}
                                value={draft[0] ?? null}
                                onValueChange={(value) => {
                                    if (typeof value === "string")
                                        choose(value, true)
                                }}
                                aria-labelledby={`${id}-tree-title`}
                                className="gap-0"
                            >
                                {tree}
                            </RadioGroup>
                        )}
                    </div>
                </section>
                {multiple && (
                    <section
                        className="flex min-h-0 flex-col border-t border-border/60 pt-4 sm:border-t-0 sm:border-l sm:py-4 sm:pl-5"
                        aria-labelledby={`${id}-selected-title`}
                    >
                        <div className="mb-2.5 flex items-center justify-between gap-2">
                            <h3
                                id={`${id}-selected-title`}
                                className="font-semibold"
                            >
                                已选人员{" "}
                                <span className="num ml-1 rounded-full bg-muted px-2 py-0.5 text-xs font-normal text-muted-foreground">
                                    {draft.length}
                                </span>
                            </h3>
                            <Button
                                id={`${id}-clear`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                disabled={!draft.length}
                                onClick={() => setDraft([])}
                            >
                                {listFilterText.clear}
                            </Button>
                        </div>
                        <div className="max-h-60 min-h-0 overflow-auto sm:h-[min(24rem,45dvh)] sm:max-h-none">
                            {draft.length > 0 ? (
                                <ul className="divide-y divide-border/60">
                                    {draft.map((personId) => {
                                        const person = model.byId.get(personId)
                                        const name =
                                            person?.name ||
                                            (busy
                                                ? "正在核对人员"
                                                : "人员信息不可用")
                                        return (
                                            <li
                                                key={personId}
                                                className="flex items-center gap-2.5 py-2"
                                            >
                                                <Avatar
                                                    size="sm"
                                                    aria-hidden="true"
                                                >
                                                    <AvatarFallback>
                                                        {person?.name.slice(
                                                            0,
                                                            1,
                                                        ) || (
                                                            <UsersIcon className="size-3.5" />
                                                        )}
                                                    </AvatarFallback>
                                                </Avatar>
                                                <div className="min-w-0 flex-1">
                                                    <p className="break-words font-medium">
                                                        {name}
                                                    </p>
                                                    <p className="mt-0.5 break-words text-xs text-muted-foreground">
                                                        {person
                                                            ? model.departmentLabel(
                                                                  person,
                                                              )
                                                            : busy
                                                              ? "人员信息加载后可核对"
                                                              : "请移除后重新选择"}
                                                    </p>
                                                    {person?.disabled && (
                                                        <p className="mt-0.5 text-xs text-destructive">
                                                            {person.disabledReason ||
                                                                "不可选择"}
                                                        </p>
                                                    )}
                                                </div>
                                                <Button
                                                    id={itemId(
                                                        "remove",
                                                        personId,
                                                    )}
                                                    type="button"
                                                    variant="ghost"
                                                    size="icon-xs"
                                                    aria-label={`移除${name}`}
                                                    onClick={() =>
                                                        remove(personId)
                                                    }
                                                >
                                                    <XIcon aria-hidden="true" />
                                                </Button>
                                            </li>
                                        )
                                    })}
                                </ul>
                            ) : (
                                <BusinessEmptyState
                                    kind="no-data"
                                    icon={UsersIcon}
                                    title="尚未选择人员"
                                    description="请从组织架构中选择人员"
                                    className="py-8"
                                />
                            )}
                        </div>
                    </section>
                )}
            </DialogBody>
            {!busy && invalidSelection && (
                <p
                    role="alert"
                    className="shrink-0 border-t border-destructive-border bg-destructive-soft/75 px-overlay-padding py-2 text-xs text-destructive-soft-foreground"
                >
                    部分已选人员不可用，请移除或重新选择后确认。
                </p>
            )}
            <DialogFooter className="flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <div
                    className="min-w-0 flex-1 text-body-compact"
                    aria-live="polite"
                >
                    {multiple ? (
                        <span>
                            已选择{" "}
                            <span className="num font-medium">
                                {draft.length}
                            </span>{" "}
                            人
                        </span>
                    ) : draft.length ? (
                        <div className="flex items-center gap-2">
                            <div className="min-w-0">
                                <p className="break-words font-medium">
                                    已选：
                                    {singlePerson?.name ||
                                        (busy
                                            ? "正在核对人员"
                                            : "人员信息不可用")}
                                </p>
                                {singlePerson && (
                                    <p className="mt-0.5 break-words text-xs text-muted-foreground">
                                        {model.departmentLabel(singlePerson)}
                                    </p>
                                )}
                            </div>
                            <Button
                                id={`${id}-clear`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                aria-label="清除已选人员"
                                onClick={() => setDraft([])}
                            >
                                <XIcon aria-hidden="true" />
                            </Button>
                        </div>
                    ) : (
                        <span className="text-muted-foreground">
                            尚未选择人员
                        </span>
                    )}
                </div>
                <div className="ml-auto flex shrink-0 items-center gap-2">
                    <Button
                        id={`${id}-cancel`}
                        type="button"
                        variant="outline"
                        onClick={() => props.onOpenChange(false)}
                    >
                        {listFilterText.cancel}
                    </Button>
                    <Button
                        id={`${id}-confirm`}
                        type="button"
                        disabled={!canConfirm}
                        onClick={() => {
                            if (!canConfirm) return
                            if (props.mode === "multiple")
                                props.onConfirm([...draft])
                            else props.onConfirm(draft[0] ?? null)
                            props.onOpenChange(false)
                        }}
                    >
                        {multiple ? (
                            <>
                                确认选择（
                                <span className="num">{draft.length}</span>）
                            </>
                        ) : (
                            "确认选择"
                        )}
                    </Button>
                </div>
            </DialogFooter>
        </DialogContent>
    )
}
