import {
    defaultValidationLogic,
    type AnyFormApi,
    type ValidationLogicFn,
} from "@tanstack/react-form"
import { getErrorPresentation, isApiError } from "@/lib/api/errors"
import { fieldErrorMessages, formFieldPath } from "@/lib/api/field-errors"

type ApiFormState = {
    failure?: ReturnType<typeof getErrorPresentation>
    issues: Record<string, { name: string; message: string }>
    summary?: string
    listeners: Set<() => void>
}

// 摘要是展示状态，不参与表单有效性；字段错误仍由 TanStack Form 管理。
const states = new WeakMap<AnyFormApi["store"], ApiFormState>()
const wrappedLogic = new WeakSet<ValidationLogicFn>()

function stateFor(form: AnyFormApi): ApiFormState {
    let state = states.get(form.store)
    if (!state) {
        state = { issues: {}, listeners: new Set() }
        states.set(form.store, state)
        const reset = form.reset
        form.reset = (...args) => {
            clearApiFormError(form)
            reset(...args)
        }
    }
    return state
}

function publish(state: ApiFormState): void {
    const fields = Object.fromEntries(
        Object.entries(state.issues).map(([path, issue]) => [
            path,
            issue.message,
        ]),
    )
    state.summary = state.failure
        ? [
              state.failure.description,
              ...fieldErrorMessages(fields),
              state.failure.requestId
                  ? `请求编号：${state.failure.requestId}`
                  : undefined,
          ]
              .filter(Boolean)
              .join("\n")
        : undefined
    state.listeners.forEach((listener) => listener())
}

export function getApiFormError(form: AnyFormApi): string | undefined {
    return states.get(form.store)?.summary
}

export function subscribeApiFormError(
    form: AnyFormApi,
    listener: () => void,
): () => void {
    const state = stateFor(form)
    state.listeners.add(listener)
    return () => {
        state.listeners.delete(listener)
    }
}

/** 保留原有同步、异步和动态校验，只跳过会批量清空服务端字段错误的校验器。 */
export function preserveApiFieldErrors(
    logic: ValidationLogicFn = defaultValidationLogic,
): ValidationLogicFn {
    if (wrappedLogic.has(logic)) return logic
    const wrapped: ValidationLogicFn = (props) =>
        logic({
            ...props,
            runValidation: (validation) =>
                props.runValidation({
                    ...validation,
                    validators: Object.keys(
                        states.get(props.form.store)?.issues ?? {},
                    ).length
                        ? validation.validators.filter(
                              (validator) => validator?.cause !== "server",
                          )
                        : validation.validators,
                }),
        })
    wrappedLogic.add(wrapped)
    return wrapped
}

function registeredPath(
    form: AnyFormApi,
    path: string,
    aliases: Readonly<Record<string, string>>,
): string {
    // 显式映射优先，其次原始字段名，最后才尝试 snake_case → camelCase。
    const explicit = aliases[path]
    if (explicit) return explicit
    if (
        Object.keys(aliases).some((part) => path.split(/[.[]/).includes(part))
    ) {
        const mapped = formFieldPath(path, aliases)
        if (form.fieldInfo[mapped]) return mapped
    }
    return form.fieldInfo[path] ? path : formFieldPath(path, aliases)
}

/** 绑定字段错误并保留摘要；普通拒绝不影响 state.canSubmit。 */
export function applyApiFormError(
    form: AnyFormApi,
    error: unknown,
    aliases: Readonly<Record<string, string>> = {},
): void {
    const state = stateFor(form)
    clearApiFormError(form)
    state.failure = getErrorPresentation(error)
    const fields: Record<string, string> = {}
    if (isApiError(error)) {
        for (const [path, message] of Object.entries(error.fieldErrors ?? {})) {
            const name = registeredPath(form, path, aliases)
            state.issues[path] = { name, message }
            if (form.fieldInfo[name]) fields[name] = message
        }
    }
    form.update({
        ...form.options,
        validationLogic: preserveApiFieldErrors(form.options.validationLogic),
    })
    form.setErrorMap({ onServer: { fields } })
    for (const name of Object.keys(fields)) {
        form.setFieldMeta(name, (meta) => ({ ...meta, isTouched: true }))
    }
    publish(state)
    const first = Object.keys(fields)[0]
    if (first && typeof document !== "undefined") {
        document
            .querySelector<HTMLElement>(`[name="${CSS.escape(first)}"]`)
            ?.focus()
    }
}

/** 仅清理实际修改字段及其子字段，未修改字段的摘要和校验状态继续保留。 */
export function clearApiFieldError(form: AnyFormApi, name: string): void {
    const state = states.get(form.store)
    if (!state) return
    let changed = false
    for (const [path, issue] of Object.entries(state.issues)) {
        if (
            issue.name !== name &&
            !issue.name.startsWith(`${name}.`) &&
            !issue.name.startsWith(`${name}[`)
        )
            continue
        if (form.getFieldMeta(issue.name)) {
            form.setFieldMeta(issue.name, (meta) => ({
                ...meta,
                errorMap: { ...meta.errorMap, onServer: undefined },
            }))
        }
        delete state.issues[path]
        changed = true
    }
    if (!changed) return
    if (Object.keys(state.issues).length === 0) state.failure = undefined
    publish(state)
}

/** 新提交、成功或重置时移除上次请求的展示状态。 */
export function clearApiFormError(form: AnyFormApi): void {
    const state = states.get(form.store)
    if (!state) return
    for (const issue of Object.values(state.issues)) {
        if (form.getFieldMeta(issue.name)) {
            form.setFieldMeta(issue.name, (meta) => ({
                ...meta,
                errorMap: { ...meta.errorMap, onServer: undefined },
            }))
        }
    }
    state.issues = {}
    state.failure = undefined
    publish(state)
}
