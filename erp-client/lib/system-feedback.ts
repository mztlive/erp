"use client"

import { getErrorPresentation, type ErrorPresentation } from "@/lib/api/errors"
import { fieldErrorMessages } from "@/lib/api/field-errors"
import { isWorkspaceFrame, tabHref } from "@/lib/workspace-tabs"

export type SystemProblem = {
    key: string
    href: string
    page: string
    title: string
    description: string
    technical: string
    fields: string[]
    kind: ErrorPresentation["kind"]
    source: Window | null
    retry?: () => Promise<unknown>
    operation?: boolean
}
type FeedbackState = { problems: SystemProblem[]; open: boolean }
type FeedbackStore = {
    get: () => FeedbackState
    subscribe: (listener: () => void) => () => void
    report: (problem: SystemProblem, notify: boolean) => void
    resolve: (key: string) => void
    clear: () => void
    forgetSource: (source: Window) => void
    open: (open: boolean) => void
    notify?: (problems: SystemProblem[]) => void
}
declare global {
    interface Window {
        erpSystemFeedback?: FeedbackStore
    }
}
export const emptyFeedback: FeedbackState = { problems: [], open: false }

/** 同源工作页共用宿主内存；不将错误、单据内容或重试闭包写入持久存储。 */
export function feedbackStore(): FeedbackStore {
    const owner = isWorkspaceFrame() ? window.parent : window
    if (owner.erpSystemFeedback) return owner.erpSystemFeedback
    let state: FeedbackState = emptyFeedback
    const listeners = new Set<() => void>()
    let pending: SystemProblem[] = []
    let timer: number | undefined
    const emit = () => listeners.forEach((listener) => listener())
    const store: FeedbackStore = {
        get: () => state,
        subscribe: (listener) => {
            listeners.add(listener)
            return () => {
                listeners.delete(listener)
            }
        },
        report: (problem, notify) => {
            const previous = state.problems.find(
                (item) => item.key === problem.key,
            )
            state = {
                ...state,
                problems: [
                    ...state.problems.filter(
                        (item) => item.key !== problem.key,
                    ),
                    problem,
                ],
            }
            emit()
            if (
                notify &&
                (!previous || previous.description !== problem.description)
            ) {
                pending.push(problem)
                if (!timer)
                    timer = owner.setTimeout(() => {
                        timer = undefined
                        const batch = [
                            ...new Set(pending.map((item) => item.key)),
                        ]
                            .map((key) =>
                                state.problems.find((item) => item.key === key),
                            )
                            .filter((item): item is SystemProblem =>
                                Boolean(item),
                            )
                        pending = []
                        if (batch.length) store.notify?.(batch)
                    }, 350)
            }
        },
        resolve: (key) => {
            if (!state.problems.some((item) => item.key === key)) return
            state = {
                ...state,
                problems: state.problems.filter((item) => item.key !== key),
            }
            emit()
        },
        forgetSource: (source) => {
            state = {
                ...state,
                problems: state.problems
                    .filter((item) => item.source !== source || item.operation)
                    .map((item) =>
                        item.source === source
                            ? { ...item, source: null, retry: undefined }
                            : item,
                    ),
            }
            emit()
        },
        clear: () => {
            if (timer) owner.clearTimeout(timer)
            timer = undefined
            pending = []
            state = emptyFeedback
            emit()
        },
        open: (open) => {
            state = { ...state, open }
            emit()
        },
    }
    owner.erpSystemFeedback = store
    return store
}

export function problemKey(key: string): string {
    return `${window.location.pathname}:${key}`
}

/** 仅数据读取提供 retry；写操作只能返回来源页面核对结果。 */
export function reportProblem(
    key: string,
    error: unknown,
    options: {
        title?: string
        description?: string
        retry?: () => Promise<unknown>
        notify?: boolean
        operation?: boolean
        presentation?: ErrorPresentation
    } = {},
) {
    const failure = options.presentation ?? getErrorPresentation(error)
    const permission = failure.kind === "permission"
    const href = tabHref(new URL(window.location.href))
    feedbackStore().report(
        {
            key: problemKey(key),
            href,
            page: [
                document.querySelector("h1")?.textContent?.trim() ||
                    document.title.replace(/ · 福尚云 ERP$/, ""),
                document
                    .querySelector("[data-workspace-document-number]")
                    ?.getAttribute("data-workspace-document-number"),
            ]
                .filter(Boolean)
                .join(" · "),
            title:
                options.title ??
                (permission ? "部分资料无权查看" : failure.title),
            description:
                options.description ??
                (permission
                    ? "当前账号无权查看这部分资料，其他已加载内容仍可使用。"
                    : failure.description),
            fields: fieldErrorMessages(failure.fieldErrors),
            technical: [
                failure.description,
                failure.code && `错误码：${failure.code}`,
                failure.requestId && `请求编号：${failure.requestId}`,
            ]
                .filter(Boolean)
                .join("\n"),
            kind: failure.kind,
            source: window,
            retry: failure.retryable ? options.retry : undefined,
            operation: options.operation,
        },
        options.notify !== false &&
            (!permission || options.operation === true) &&
            (!isWorkspaceFrame() ||
                window.frameElement?.getAttribute("data-workspace-active") ===
                    "true"),
    )
}
export function resolveProblem(key: string) {
    feedbackStore().resolve(problemKey(key))
}
