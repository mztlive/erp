"use client"

import { useState } from "react"
import Link from "next/link"
import {
    ArrowUpRightIcon,
    CalendarDaysIcon,
    CheckIcon,
    EllipsisIcon,
    RefreshCwIcon,
} from "lucide-react"
import { z } from "zod"
import { PageHeader } from "@/components/business"
import { useAppFilterForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
    Popover,
    PopoverContent,
    PopoverDescription,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import type { AccountProfile } from "@/features/auth/api"
import { ROLE_WORKBENCHES, type roleWorkbench } from "@/lib/role-workbenches"
import { useRouter } from "@/lib/navigation"
import { savePreference } from "../pages/role-workbench-entry"
import type { WorkspaceOverview } from "./workspace-overview"
import { WORKBENCH_PRESENTATION } from "../lib/workbench-presentation"
import { WorkbenchDataInfo, workbenchUpdatedAt } from "./workbench-data-info"

const dateSchema = z.iso.date("请选择有效日期")
const rangeSchema = z
    .object({ from: dateSchema, to: dateSchema })
    .refine((value) => value.from <= value.to, {
        message: "结束日不得早于开始日",
        path: ["to"],
    })

export function RoleWorkbenchHeader({
    template,
    profile,
    periodQuery,
    onRefresh,
    refreshing,
}: {
    template: NonNullable<ReturnType<typeof roleWorkbench>>
    profile: AccountProfile
    periodQuery: string
    onRefresh: () => void
    refreshing: boolean
}) {
    const router = useRouter()
    const [saved, setSaved] = useState<boolean | null>(null)
    const workbenches = ROLE_WORKBENCHES.filter((item) =>
        profile.workbenches?.includes(item.id),
    )
    return (
        <PageHeader
            className="static bg-transparent"
            title={template.name}
            description={WORKBENCH_PRESENTATION[template.id].description}
            actions={
                <div className="flex flex-wrap items-center gap-2">
                    {workbenches.length > 1 && (
                        <NativeSelect
                            id="role-workbench-switch"
                            aria-label="切换岗位"
                            value={template.id}
                            onChange={(event) =>
                                router.push(
                                    `/workspace/roles/${event.target.value}?${periodQuery}`,
                                )
                            }
                        >
                            {workbenches.map((item) => (
                                <NativeSelectOption
                                    key={item.id}
                                    value={item.id}
                                >
                                    {item.name}
                                </NativeSelectOption>
                            ))}
                        </NativeSelect>
                    )}
                    <Button
                        id="role-workbench-refresh"
                        variant="outline"
                        size="sm"
                        onClick={onRefresh}
                        disabled={refreshing}
                    >
                        <RefreshCwIcon
                            aria-hidden="true"
                            className={refreshing ? "animate-spin" : undefined}
                        />
                        {refreshing ? "更新中" : "刷新"}
                    </Button>
                    <Button
                        id="role-workbench-all-todos"
                        size="sm"
                        render={<Link href="/workspace/tasks" />}
                    >
                        待我处理
                        <ArrowUpRightIcon aria-hidden="true" />
                    </Button>
                    <DropdownMenu>
                        <DropdownMenuTrigger
                            id="role-workbench-more"
                            aria-label="更多工作台设置"
                            render={<Button variant="ghost" size="icon-sm" />}
                        >
                            <EllipsisIcon aria-hidden="true" />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end" className="w-44">
                            <DropdownMenuItem
                                id="role-workbench-default"
                                onClick={() =>
                                    setSaved(
                                        savePreference(
                                            profile.userid,
                                            template.id,
                                            true,
                                        ),
                                    )
                                }
                            >
                                {saved === true && (
                                    <CheckIcon aria-hidden="true" />
                                )}
                                设为默认首页
                            </DropdownMenuItem>
                        </DropdownMenuContent>
                    </DropdownMenu>
                    {saved !== null && (
                        <span
                            role="status"
                            className={
                                saved
                                    ? "text-xs text-muted-foreground"
                                    : "text-xs text-destructive"
                            }
                        >
                            {saved ? "已设为默认首页" : "保存失败，请重试"}
                        </span>
                    )}
                </div>
            }
        />
    )
}

export function SalesWorkbenchPeriod({
    overview,
    custom,
}: {
    overview: WorkspaceOverview
    custom?: { from: string; to: string }
}) {
    const router = useRouter()
    const [open, setOpen] = useState(false)
    const form = useAppFilterForm({
        defaultValues: custom ?? overview.period,
        validators: { onChange: rangeSchema, onSubmit: rangeSchema },
        onSubmit: ({ value }) => {
            router.replace(
                `/workspace/roles/WB-01?from=${value.from}&to=${value.to}`,
            )
            setOpen(false)
        },
    })
    return (
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
            <div className="flex items-center gap-1.5">
                <h2 className="text-section-title font-semibold tracking-tight">
                    业务概览
                </h2>
                <WorkbenchDataInfo
                    id="sales-workbench-scope-info"
                    title="业务概览"
                >
                    <p>
                        各项业务统计分别按对应模块的授权范围展示，具体范围见指标详情。
                    </p>
                    <p>
                        期间筛选只影响新增销售单；审批中销售单、待销售处理询价与应收未收均为截至当前的数据。
                    </p>
                    <p>待办只包含本人具备处理资格的待办任务，不随期间变化。</p>
                </WorkbenchDataInfo>
                <span className="hidden text-xs text-muted-foreground sm:inline">
                    ·{" "}
                    {overview.queries.some(
                        (query) => query.isFetching && !query.isPending,
                    )
                        ? "更新中…"
                        : workbenchUpdatedAt(
                              overview.queries.map((query) =>
                                  query.isError ? undefined : query.data?.asOf,
                              ),
                          )}
                </span>
            </div>
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <label htmlFor="role-workbench-period">新增单据期间</label>
                <NativeSelect
                    id="role-workbench-period"
                    aria-describedby="sales-workbench-period-description"
                    value={custom ? "custom" : overview.preset}
                    onChange={(event) => {
                        if (event.target.value === "custom") {
                            form.reset(custom ?? overview.period)
                            setOpen(true)
                        } else
                            router.replace(
                                `/workspace/roles/WB-01?period=${event.target.value}`,
                            )
                    }}
                >
                    <NativeSelectOption value="month-to-date">
                        本月迄今
                    </NativeSelectOption>
                    <NativeSelectOption value="last-month">
                        上月
                    </NativeSelectOption>
                    <NativeSelectOption value="quarter-to-date">
                        本季迄今
                    </NativeSelectOption>
                    <NativeSelectOption value="custom">
                        自定义期间
                    </NativeSelectOption>
                </NativeSelect>
                <Popover
                    open={open}
                    onOpenChange={(next) => {
                        if (next) form.reset(custom ?? overview.period)
                        setOpen(next)
                    }}
                >
                    <PopoverTrigger
                        id="sales-workbench-custom-period"
                        aria-label="设置自定义统计期间"
                        render={<Button variant="ghost" size="sm" />}
                    >
                        <CalendarDaysIcon aria-hidden="true" />
                        {overview.period.from} 至 {overview.period.to}
                    </PopoverTrigger>
                    <PopoverContent
                        align="end"
                        className="w-80 max-w-[calc(100vw-2rem)]"
                    >
                        <PopoverTitle className="text-section-title font-semibold tracking-tight">
                            自定义新增单据期间
                        </PopoverTitle>
                        <PopoverDescription className="text-xs">
                            按创建日期统计，包含起止日；当前余额与待办不受影响。
                        </PopoverDescription>
                        <form
                            onSubmit={(event) => {
                                event.preventDefault()
                                void form.handleSubmit()
                            }}
                            className="space-y-3"
                        >
                            <form.AppField name="from">
                                {(field) => (
                                    <div className="space-y-1.5">
                                        <label
                                            htmlFor="role-period-from"
                                            className="text-xs"
                                        >
                                            开始日
                                        </label>
                                        <Input
                                            id="role-period-from"
                                            type="date"
                                            value={field.state.value}
                                            aria-invalid={
                                                field.state.meta.errors.length >
                                                0
                                            }
                                            onBlur={field.handleBlur}
                                            onChange={(event) =>
                                                field.handleChange(
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </div>
                                )}
                            </form.AppField>
                            <form.AppField name="to">
                                {(field) => (
                                    <div className="space-y-1.5">
                                        <label
                                            htmlFor="role-period-to"
                                            className="text-xs"
                                        >
                                            结束日
                                        </label>
                                        <Input
                                            id="role-period-to"
                                            type="date"
                                            value={field.state.value}
                                            aria-invalid={
                                                field.state.meta.errors.length >
                                                0
                                            }
                                            aria-describedby="role-period-error"
                                            onBlur={field.handleBlur}
                                            onChange={(event) =>
                                                field.handleChange(
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </div>
                                )}
                            </form.AppField>
                            <form.Subscribe selector={(state) => state.values}>
                                {(values) => {
                                    const valid = rangeSchema.safeParse(values)
                                    return (
                                        <>
                                            <p
                                                id="role-period-error"
                                                role="status"
                                                className="text-xs text-destructive"
                                            >
                                                {!valid.success
                                                    ? valid.error.issues[0]
                                                          ?.message
                                                    : null}
                                            </p>
                                            <Button
                                                id="role-period-apply"
                                                type="submit"
                                                size="sm"
                                                className="w-full"
                                                disabled={!valid.success}
                                            >
                                                应用期间
                                            </Button>
                                        </>
                                    )
                                }}
                            </form.Subscribe>
                        </form>
                    </PopoverContent>
                </Popover>
                <span
                    id="sales-workbench-period-description"
                    className="sr-only"
                >
                    仅筛选新增销售单，其他指标截至当前。
                </span>
            </div>
        </div>
    )
}
