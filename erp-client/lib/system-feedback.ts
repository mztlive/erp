"use client"

import {
    getErrorPresentation,
    isApiError,
    type ErrorPresentation,
} from "@/lib/api/errors"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { fieldErrorMessages } from "@/lib/api/field-errors"
import { resultText, systemFeedbackText } from "@/lib/ui-text"
import { isWorkspaceFrame, tabHref } from "@/lib/workspace-tabs"

export type SystemProblem = {
    key: string
    href: string
    page: string
    subject?: string
    title: string
    description: string
    technicalReason?: string
    errorCode?: string
    requestId?: string
    responseStatus?: number
    reportedAt: string
    requiredPermission?: string
    permissionName?: string
    fingerprint: string
    fields: string[]
    accessDenied: boolean
    source: Window | null
    retry?: () => Promise<unknown>
    operation?: boolean
    outcomeUnknown?: boolean
}

/** 同一份可转发文本用于复制及手动复制；只读取已整理的提示字段。 */
export function formatProblemReport(problem: SystemProblem): string {
    const singleLine = (value: string) => value.replace(/\s+/g, " ").trim()
    const line = (label: string, value?: string | number) => {
        const text = singleLine(String(value ?? ""))
        return text ? `${label}：${text}` : undefined
    }
    const summary = [
        line("ERP 系统提示", problem.title),
        line(systemFeedbackText.source, problem.page),
        line("受影响内容", problem.subject),
        line("说明", problem.description),
    ]
    const fields = problem.fields.map(singleLine).filter(Boolean)
    const validation = fields.length
        ? ["", "【字段校验】", ...fields.map((field) => `- ${field}`)]
        : []
    const permission = problem.accessDenied
        ? [
              "",
              "【管理员核查】",
              line("权限名称", problem.permissionName),
              line(
                  systemFeedbackText.requiredPermission,
                  problem.requiredPermission,
              ),
              line(
                  "核查项",
                  problem.requiredPermission
                      ? "核对报错账号的启用角色是否授予该权限，以及数据范围是否覆盖受影响的内容。"
                      : "按来源工作页和受影响内容核对报错账号的启用角色与数据范围。",
              ),
          ]
        : []
    const diagnostics = [
        "",
        "【开发定位】",
        line(systemFeedbackText.requestId, problem.requestId),
        line(systemFeedbackText.errorCode, problem.errorCode),
        line("响应状态", problem.responseStatus),
        line("记录时间（UTC）", problem.reportedAt),
        line(
            "补充说明",
            problem.requiredPermission ? undefined : problem.technicalReason,
        ),
    ]
    return [...summary, ...validation, ...permission, ...diagnostics]
        .filter((value) => value !== undefined)
        .join("\n")
}

/** 由业务查询提供用户看得见的内容名称和已知影响。 */
export type ReadProblemContext = {
    subject: string
    impact?: string
}

function permissionDescription(detail?: string, impact?: string): string {
    return [detail, systemFeedbackText.permissionHelp, impact]
        .filter(Boolean)
        .join(" ")
}

/** 按读取对象组织提示，登录失效沿用重新登录指引。 */
export function readProblemCopy(
    failure: ErrorPresentation,
    context?: ReadProblemContext,
    refreshFailed = false,
) {
    const subject = context?.subject
    if (failure.accessDenied) {
        return {
            title: subject ? `无权查看${subject}` : "当前账号无权查看此项内容",
            description: permissionDescription(
                failure.accessDenied.detail,
                context?.impact,
            ),
        }
    }
    if (failure.kind === "permission") return failure
    return {
        title: `${subject ?? "页面数据"}${refreshFailed ? "刷新失败" : "加载失败"}`,
        description: [failure.description, context?.impact]
            .filter(Boolean)
            .join(" "),
    }
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
        outcomeUnknown?: boolean
        readContext?: ReadProblemContext
        refreshFailed?: boolean
        presentation?: ErrorPresentation
    } = {},
) {
    const failure =
        options.presentation ?? getErrorPresentation(error, "请稍后重新加载。")
    const copy = options.operation
        ? failure
        : readProblemCopy(failure, options.readContext, options.refreshFailed)
    const title = options.title ?? copy.title
    const description = options.description ?? copy.description
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
            subject: options.readContext?.subject,
            title,
            description,
            fields: fieldErrorMessages(failure.fieldErrors),
            fingerprint: JSON.stringify([
                failure.kind,
                failure.code,
                failure.accessDenied?.requiredPermission,
                title,
                description,
                failure.fieldErrors,
            ]),
            technicalReason:
                failure.description !== description
                    ? failure.description
                    : undefined,
            errorCode: failure.code,
            requestId: failure.requestId,
            responseStatus: isApiError(error) ? error.status : undefined,
            reportedAt: new Date().toISOString(),
            requiredPermission: failure.accessDenied?.requiredPermission,
            permissionName: failure.accessDenied?.permissionName,
            accessDenied: Boolean(failure.accessDenied),
            source: window,
            retry:
                !options.operation && failure.retryable
                    ? options.retry
                    : undefined,
            operation: options.operation,
            outcomeUnknown: options.outcomeUnknown,
        },
        options.notify !== false &&
            (failure.kind !== "permission" || options.operation === true) &&
            (!isWorkspaceFrame() ||
                window.frameElement?.getAttribute("data-workspace-active") ===
                    "true"),
    )
    return problemKey(key)
}

/** 同一次异常在请求层和业务层共用记录；不同操作即使错误码相同也不互相覆盖。 */
export function reportOperationProblem(
    error: unknown,
    options: {
        key?: string
        title?: string
        description?: string
        outcomeUnknown?: boolean
    } = {},
) {
    const failure = getErrorPresentation(error)
    const committed = failure.code === "COMMITTED_RESULT_UNAVAILABLE"
    const outcomeUnknown =
        !committed &&
        (options.outcomeUnknown ??
            (isApiError(error) &&
                commandFailureDisposition(error) === "unknown"))
    let title = options.title ?? failure.title
    let description =
        options.description ??
        (failure.accessDenied
            ? permissionDescription(failure.accessDenied.detail)
            : failure.description)
    if (committed) {
        title = systemFeedbackText.committedResultUnavailable
        description = systemFeedbackText.checkCommittedResult
    } else if (outcomeUnknown) {
        title = options.title ?? resultText.unknown
        description = options.description ?? systemFeedbackText.verifyOperation
    }
    return reportProblem(
        feedbackStore().operationKey(error, options.key),
        error,
        {
            operation: true,
            outcomeUnknown,
            presentation: failure,
            title,
            description,
        },
    )
}

export function resolveProblem(key: string) {
    feedbackStore().resolve(problemKey(key))
}
