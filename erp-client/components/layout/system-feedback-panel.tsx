"use client"

import { useEffect, useState, useSyncExternalStore } from "react"
import { usePathname } from "next/navigation"
import { CircleAlertIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    Sheet,
    SheetContent,
    SheetHeader,
    SheetTitle,
    SheetDescription,
} from "@/components/ui/sheet"
import { toast } from "@/components/ui/toast"
import {
    emptyFeedback,
    feedbackStore,
    type SystemProblem,
} from "@/lib/system-feedback"
import { workspaceHost } from "@/lib/workspace-tabs"
import { toAutomationIdSegment } from "@/lib/automation-id"

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
                        ? "relative text-foreground hover:text-foreground"
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
                <CircleAlertIcon className="size-4" aria-hidden />
                <span className="sr-only">系统提示</span>
                {problemCount > 0 ? (
                    <span className="num pointer-events-none absolute top-0.5 right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-destructive px-1 text-2xs font-semibold leading-none text-destructive-foreground ring-2 ring-card">
                        {problemCount > 99 ? "99+" : problemCount}
                    </span>
                ) : null}
            </Button>
            <SheetContent size="preview" closeButtonId="system-feedback-close">
                <SheetHeader>
                    <SheetTitle>系统提示</SheetTitle>
                    <SheetDescription>
                        读取问题恢复后自动移除；操作结果需回到来源页面核对。
                    </SheetDescription>
                </SheetHeader>
                <div className="flex gap-2 border-b px-7 pb-3">
                    <Button
                        id="system-feedback-current"
                        size="sm"
                        variant={all ? "ghost" : "secondary"}
                        onClick={() => setAll(false)}
                    >
                        当前工作页
                    </Button>
                    <Button
                        id="system-feedback-all"
                        size="sm"
                        variant={all ? "secondary" : "ghost"}
                        onClick={() => setAll(true)}
                    >
                        本次会话
                    </Button>
                </div>
                <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-7 py-4">
                    {problems.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            {all
                                ? "当前没有待处理的系统提示。"
                                : "当前工作页没有系统提示。"}
                        </p>
                    ) : (
                        problems.map((problem) => (
                            <ProblemCard key={problem.key} problem={problem} />
                        ))
                    )}
                </div>
            </SheetContent>
        </Sheet>
    )
}

function ProblemCard({ problem }: { problem: SystemProblem }) {
    const [busy, setBusy] = useState(false)
    const [message, setMessage] = useState("")
    const id = `system-feedback-${toAutomationIdSegment(encodeURIComponent(problem.key))}`
    return (
        <section className="space-y-2 rounded-lg border p-3">
            <p className="text-xs text-muted-foreground">{problem.page}</p>
            <h3 className="text-sm font-medium">{problem.title}</h3>
            <p className="text-sm text-muted-foreground">
                {problem.description}
            </p>
            {problem.fields?.length > 0 && (
                <ul className="list-inside list-disc space-y-1 text-sm">
                    {problem.fields.map((field) => (
                        <li key={field}>{field}</li>
                    ))}
                </ul>
            )}
            <div className="flex flex-wrap gap-2">
                <Button
                    id={`${id}-source`}
                    size="sm"
                    variant="outline"
                    onClick={() => {
                        feedbackStore().open(false)
                        workspaceHost()?.navigate(null, problem.href)
                    }}
                >
                    打开来源页面
                </Button>
                {problem.retry && (
                    <Button
                        id={`${id}-retry`}
                        size="sm"
                        variant="outline"
                        disabled={busy}
                        onClick={async () => {
                            setBusy(true)
                            setMessage("")
                            try {
                                await problem.retry?.()
                            } catch {
                                setMessage("仍未恢复，请稍后再试。")
                            } finally {
                                setBusy(false)
                            }
                        }}
                    >
                        {busy ? "正在重试…" : "重新加载"}
                    </Button>
                )}
            </div>
            {problem.operation && (
                <Button
                    id={`${id}-acknowledge`}
                    size="sm"
                    variant="ghost"
                    onClick={() => feedbackStore().resolve(problem.key)}
                >
                    已核对结果，移除此提示
                </Button>
            )}
            {message && (
                <p role="status" className="text-sm text-muted-foreground">
                    {message}
                </p>
            )}
            <details>
                <summary
                    id={`${id}-technical`}
                    className="cursor-pointer text-xs text-muted-foreground"
                >
                    技术详情
                </summary>
                <p className="mt-2 text-xs whitespace-pre-wrap break-words">
                    {problem.technical}
                </p>
                <Button
                    id={`${id}-copy`}
                    size="sm"
                    variant="ghost"
                    onClick={async () => {
                        try {
                            await navigator.clipboard.writeText(
                                `${problem.page}\n${problem.title}\n${problem.technical}`,
                            )
                            setMessage("问题信息已复制")
                        } catch {
                            setMessage("复制失败，请手动选择技术详情复制。")
                        }
                    }}
                >
                    复制问题信息
                </Button>
            </details>
        </section>
    )
}
