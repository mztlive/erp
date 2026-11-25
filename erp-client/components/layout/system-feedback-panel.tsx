"use client"

import { useEffect, useRef, useState, useSyncExternalStore } from "react"
import { usePathname } from "next/navigation"
import {
    ArrowUpRightIcon,
    CheckIcon,
    ChevronDownIcon,
    CircleAlertIcon,
    CopyIcon,
    ShieldAlertIcon,
    TriangleAlertIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    Sheet,
    SheetContent,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet"
import { toast } from "@/components/ui/toast"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import {
    emptyFeedback,
    feedbackStore,
    formatProblemReport,
    type SystemProblem,
} from "@/lib/system-feedback"
import { workspaceHost } from "@/lib/workspace-tabs"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { systemFeedbackText } from "@/lib/ui-text"

const subscribe = (listener: () => void) => feedbackStore().subscribe(listener)
const snapshot = () => feedbackStore().get()

/** 系统错误独立于业务收件箱；关闭面板不清除未恢复的问题。 */
export function SystemFeedbackPanel() {
    const state = useSyncExternalStore(subscribe, snapshot, () => emptyFeedback)
    const pathname = usePathname()
    const [all, setAll] = useState(false)
    useEffect(() => {
        const store = feedbackStore()
        const batches = new Map<string, string[]>()
        store.notify = (batch) => {
            const id = toast.add({
                title:
                    batch.length === 1
                        ? batch[0].title
                        : `${batch.length} 项内容需要查看`,
                description:
                    batch.length === 1
                        ? batch[0].description
                        : "请查看影响范围和处理方式，当前页面已保留。",
                type: "error",
                timeout: 6000,
                onClose: () => {
                    batches.delete(id)
                },
                actionProps: {
                    children: "查看详情",
                    onClick: () => {
                        setAll(true)
                        store.open(true)
                    },
                },
            })
            batches.set(
                id,
                batch.map((problem) => problem.key),
            )
        }
        const unsubscribe = store.subscribe(() => {
            const problems = store.get().problems
            for (const [id, keys] of batches) {
                if (!problems.some((problem) => keys.includes(problem.key))) {
                    batches.delete(id)
                    toast.close(id)
                }
            }
        })
        return () => {
            unsubscribe()
            for (const id of batches.keys()) toast.close(id)
            store.notify = undefined
        }
    }, [])
    const problems = state.problems.filter(
        (item) =>
            all ||
            new URL(item.href, window.location.origin).pathname === pathname,
    )
    const problemCount = state.problems.length
    return (
        <Sheet
            open={state.open}
            onOpenChange={(open) => feedbackStore().open(open)}
        >
            <Button
                id="system-feedback-open"
                variant="ghost"
                size="icon-sm"
                className={
                    problemCount > 0
                        ? "relative border border-warning-border bg-warning-soft text-warning-soft-foreground hover:bg-warning-soft/80 hover:text-warning-soft-foreground"
                        : "text-muted-foreground hover:text-foreground"
                }
                aria-label={`系统提示，${problemCount} 项`}
                title={
                    problemCount > 0
                        ? `系统提示（${problemCount} 项待处理）`
                        : "系统提示"
                }
                onClick={() => {
                    setAll(false)
                    feedbackStore().open(true)
                }}
            >
                <TriangleAlertIcon className="size-4" aria-hidden />
                <span className="sr-only">系统提示</span>
                {problemCount > 0 ? (
                    <span className="num pointer-events-none absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full border border-warning-border bg-warning-soft px-1 text-tiny leading-none font-semibold text-warning-soft-foreground ring-2 ring-card">
                        {problemCount > 99 ? "99+" : problemCount}
                    </span>
                ) : null}
            </Button>
            <SheetContent size="preview" closeButtonId="system-feedback-close">
                <SheetHeader>
                    <SheetTitle>系统提示</SheetTitle>
                </SheetHeader>
                <div className="px-sheet-inline pb-4">
                    <div
                        className="grid grid-cols-2 gap-1 rounded-lg bg-muted/70 p-1"
                        role="group"
                        aria-label="提示范围"
                    >
                        <Button
                            id="system-feedback-current"
                            size="sm"
                            variant="ghost"
                            aria-pressed={!all}
                            className="text-muted-foreground focus-visible:relative focus-visible:z-10 focus-visible:border-primary focus-visible:ring-3 focus-visible:ring-ring aria-pressed:bg-card aria-pressed:text-foreground aria-pressed:shadow-xs"
                            onClick={() => setAll(false)}
                        >
                            当前工作页
                        </Button>
                        <Button
                            id="system-feedback-all"
                            size="sm"
                            variant="ghost"
                            aria-pressed={all}
                            className="text-muted-foreground focus-visible:relative focus-visible:z-10 focus-visible:border-primary focus-visible:ring-3 focus-visible:ring-ring aria-pressed:bg-card aria-pressed:text-foreground aria-pressed:shadow-xs"
                            onClick={() => setAll(true)}
                        >
                            本次会话
                        </Button>
                    </div>
                </div>
                <TooltipProvider delay={250}>
                    <div className="min-h-0 flex-1 space-y-3 overflow-y-auto border-t border-border/70 bg-muted/25 px-sheet-inline py-4">
                        {problems.length === 0 ? (
                            <p className="py-6 text-center text-body-compact text-muted-foreground">
                                {all
                                    ? "本次会话没有系统提示。"
                                    : "当前工作页没有系统提示。"}
                            </p>
                        ) : (
                            problems.map((problem) => (
                                <ProblemCard
                                    key={problem.key}
                                    problem={problem}
                                />
                            ))
                        )}
                    </div>
                </TooltipProvider>
            </SheetContent>
        </Sheet>
    )
}

function ProblemCard({ problem }: { problem: SystemProblem }) {
    const technicalDetails = useRef<HTMLDetailsElement>(null)
    const [copyResult, setCopyResult] = useState<{
        text: string
        ok: boolean
    }>()
    const [retryState, setRetryState] = useState<
        "idle" | "retrying" | "failed"
    >("idle")
    const id = `system-feedback-${toAutomationIdSegment(encodeURIComponent(problem.key))}`
    const source = `${systemFeedbackText.source} · ${problem.page}`
    const copyText = formatProblemReport(problem)
    const copied = copyResult?.text === copyText && copyResult.ok
    const copyFailed = copyResult?.text === copyText && !copyResult.ok
    const copyLabel = copied
        ? systemFeedbackText.copied
        : systemFeedbackText.copy
    const diagnostics = [
        { label: systemFeedbackText.errorCode, value: problem.errorCode },
        { label: systemFeedbackText.requestId, value: problem.requestId },
    ].filter(({ value }) => value)

    return (
        <article
            aria-labelledby={`${id}-title`}
            className="min-w-0 rounded-lg border border-border/80 bg-card shadow-xs"
        >
            <div className="space-y-3 px-4 pt-4 pb-3">
                <div className="flex items-start gap-3">
                    {problem.accessDenied ? (
                        <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-warning-soft text-warning-soft-foreground">
                            <ShieldAlertIcon className="size-4" aria-hidden />
                        </span>
                    ) : (
                        <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground">
                            <CircleAlertIcon className="size-4" aria-hidden />
                        </span>
                    )}
                    <div className="min-w-0 flex-1 space-y-1 pt-0.5">
                        <h3
                            id={`${id}-title`}
                            className="text-body-compact font-semibold wrap-anywhere"
                        >
                            {problem.title}
                        </h3>
                        <p className="text-xs wrap-anywhere text-muted-foreground">
                            {source}
                        </p>
                    </div>
                    <Tooltip>
                        <TooltipTrigger
                            id={`${id}-copy`}
                            aria-label={systemFeedbackText.copy}
                            render={
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-sm"
                                    className="text-muted-foreground"
                                />
                            }
                            onClick={async () => {
                                try {
                                    await navigator.clipboard.writeText(
                                        copyText,
                                    )
                                    setCopyResult({ text: copyText, ok: true })
                                } catch {
                                    if (technicalDetails.current)
                                        technicalDetails.current.open = true
                                    setCopyResult({ text: copyText, ok: false })
                                }
                            }}
                        >
                            {copied ? (
                                <CheckIcon aria-hidden />
                            ) : (
                                <CopyIcon aria-hidden />
                            )}
                        </TooltipTrigger>
                        <TooltipContent>{copyLabel}</TooltipContent>
                    </Tooltip>
                    <span role="status" className="sr-only">
                        {copied ? systemFeedbackText.copied : ""}
                    </span>
                </div>
                <p className="text-body-compact wrap-anywhere text-muted-foreground">
                    {problem.description}
                </p>
                {problem.requiredPermission && (
                    <dl className="flex flex-wrap items-baseline gap-x-2 gap-y-1 rounded-lg bg-muted/60 px-3 py-2 text-xs">
                        <dt className="shrink-0 font-medium text-muted-foreground">
                            {systemFeedbackText.requiredPermission}
                        </dt>
                        <dd className="min-w-0 font-mono wrap-anywhere text-foreground">
                            {problem.requiredPermission}
                        </dd>
                    </dl>
                )}
                {problem.fields.length > 0 && (
                    <ul className="list-disc space-y-1 pl-4 text-body-compact wrap-anywhere">
                        {problem.fields.map((field) => (
                            <li key={field}>{field}</li>
                        ))}
                    </ul>
                )}
                {copyFailed && (
                    <p role="status" className="text-xs text-muted-foreground">
                        复制失败，请从技术详情手动复制。
                    </p>
                )}
                {retryState === "failed" && (
                    <p role="status" className="text-xs text-muted-foreground">
                        仍未恢复，请稍后再试。
                    </p>
                )}
            </div>
            <div className="flex flex-wrap items-center gap-1 border-t border-border/70 px-2.5 py-2">
                {problem.retry && (
                    <Button
                        id={`${id}-retry`}
                        size="xs"
                        variant="secondary"
                        disabled={retryState === "retrying"}
                        onClick={async () => {
                            setRetryState("retrying")
                            try {
                                await problem.retry?.()
                                setRetryState("idle")
                            } catch {
                                setRetryState("failed")
                            }
                        }}
                    >
                        {retryState === "retrying" ? "正在重试…" : "重新加载"}
                    </Button>
                )}
                <Button
                    id={`${id}-source`}
                    size="xs"
                    variant="ghost"
                    onClick={() => {
                        feedbackStore().open(false)
                        workspaceHost()?.navigate(null, problem.href)
                    }}
                >
                    {systemFeedbackText.returnToSource}
                    <ArrowUpRightIcon data-icon="inline-end" aria-hidden />
                </Button>
                {problem.operation && (
                    <Button
                        id={`${id}-acknowledge`}
                        size="xs"
                        variant="ghost"
                        onClick={() => feedbackStore().resolve(problem.key)}
                    >
                        {problem.outcomeUnknown
                            ? "已核对结果，移除此提示"
                            : "移除此提示"}
                    </Button>
                )}
            </div>
            {(diagnostics.length > 0 ||
                problem.technicalReason ||
                copyFailed) && (
                <details
                    ref={technicalDetails}
                    open={copyFailed || undefined}
                    className="group/technical border-t border-border/70 px-4"
                >
                    <summary
                        id={`${id}-technical`}
                        className="-mx-1.5 flex cursor-pointer list-none items-center gap-1.5 rounded-md border border-transparent px-1.5 py-2.5 text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:border-primary focus-visible:ring-3 focus-visible:ring-ring [&::-webkit-details-marker]:hidden"
                    >
                        <ChevronDownIcon
                            className="size-3 -rotate-90 group-open/technical:rotate-0"
                            aria-hidden
                        />
                        {systemFeedbackText.technicalDetails}
                    </summary>
                    {copyFailed ? (
                        <p className="pb-3 text-xs wrap-anywhere whitespace-pre-wrap select-text">
                            {copyText}
                        </p>
                    ) : (
                        <div className="space-y-2 pb-3 text-xs">
                            {diagnostics.length > 0 && (
                                <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1.5">
                                    {diagnostics.map(({ label, value }) => (
                                        <div key={label} className="contents">
                                            <dt className="text-muted-foreground">
                                                {label}
                                            </dt>
                                            <dd className="font-mono wrap-anywhere select-text">
                                                {value}
                                            </dd>
                                        </div>
                                    ))}
                                </dl>
                            )}
                            {problem.technicalReason && (
                                <p className="wrap-anywhere text-muted-foreground">
                                    {problem.technicalReason}
                                </p>
                            )}
                        </div>
                    )}
                </details>
            )}
        </article>
    )
}
