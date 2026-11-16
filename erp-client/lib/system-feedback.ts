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
    fingerprint: string
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
    operationKey: (error: unknown, preferred?: string) => string
    open: (open: boolean) => void
    notify?: (problems: SystemProblem[]) => void
}
declare global {
    interface Window {
        erpSystemFeedback?: FeedbackStore
    }
}
export const emptyFeedback: FeedbackState = { problems: [], open: false }

/** 请求异常按对象身份去重；序号标识本次会话中的操作，不包含提交参数。 */
function createOperationKey(): FeedbackStore["operationKey"] {
    let sequence = 0
    const keys = new WeakMap<object, string>()
    return (error, preferred) => {
        const object =
            typeof error === "object" && error !== null ? error : undefined
        const existing = object && keys.get(object)
        if (existing) return existing
        const key = preferred ?? `operation:${++sequence}`
        if (object) keys.set(object, key)
        return key
    }
}

/** 同源工作页共用宿主内存；不将错误、单据内容或重试闭包写入持久存储。 */
export function feedbackStore(): FeedbackStore {
    const owner = isWorkspaceFrame() ? window.parent : window
    if (owner.erpSystemFeedback) {
        // 热更新期间保留现有会话提示，同时补齐新加入的能力。
        owner.erpSystemFeedback.operationKey ??= createOperationKey()
        return owner.erpSystemFeedback
    }
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
                (!previous || previous.fingerprint !== problem.fingerprint)
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
        operationKey: createOperationKey(),
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
            fingerprint: JSON.stringify([
                failure.kind,
                failure.code,
                failure.description,
                failure.fieldErrors,
            ]),
            technical: [
                failure.description,
                failure.code && `错误码：${failure.code}`,
                failure.requestId && `请求编号：${failure.requestId}`,
            ]
                .filter(Boolean)
                .join("\n"),
            kind: failure.kind,
            source: window,
            retry:
                !options.operation && failure.retryable
                    ? options.retry
                    : undefined,
            operation: options.operation,
        },
        options.notify !== false &&
            (!permission || options.operation === true) &&
            (!isWorkspaceFrame() ||
                window.frameElement?.getAttribute("data-workspace-active") ===
                    "true"),
    )
    return problemKey(key)
}

/** 同一次异常在请求层和业务层共用记录；不同操作即使错误码相同也不互相覆盖。 */
export function reportOperationProblem(
    error: unknown,
    options: { key?: string; title?: string; description?: string } = {},
) {
    const failure = getErrorPresentation(error)
    return reportProblem(
        feedbackStore().operationKey(error, options.key),
        error,
        {
            operation: true,
            title: options.title ?? "操作未完成",
            description:
                options.description ??
                (failure.kind === "system"
                    ? "暂未确认是否提交成功，请返回原单据页面核对最新状态；您刚才填写的内容仍已保留。"
                    : failure.kind === "permission"
                      ? "当前账号无权执行此操作，请联系管理员确认权限。"
                      : failure.description),
        },
    )
}

export function resolveProblem(key: string) {
    feedbackStore().resolve(problemKey(key))
}
