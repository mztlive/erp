/**
 * Schema 驱动的 URL 状态编解码器。
 *
 * 统一各 feature 的 parseXxxSearchParams / buildXxxSearchParams 模板：
 * 每个字段声明 URL key、类型（string/enum/number/boolean/array/custom）、
 * 可选读取别名、默认值与写回条件；parse 对缺失/非法值回默认，
 * build 跳过默认值并省略空参数，保证 URL 最小化。
 */

type SearchParamsLike = URLSearchParams | { get(name: string): string | null }

type GetParam = (name: string) => string | null

type ParsedUrlState = Record<string, unknown>

export type UrlFieldSpec =
    | {
          key: string
          /** 状态对象属性名；缺省为 key */
          name?: string
          aliases?: readonly string[]
          type: "string"
          /** 写回前 trim（搜索词常用） */
          trim?: boolean
      }
    | {
          key: string
          name?: string
          aliases?: readonly string[]
          type: "enum"
          values: readonly string[]
          defaultValue?: string
          /** 校验前归一化（如环境名大写） */
          normalize?: (raw: string) => string
          /** 自定义写回条件；默认跳过缺失值与 defaultValue */
          buildWhen?: (value: unknown, state: ParsedUrlState) => boolean
      }
    | {
          key: string
          name?: string
          type: "number"
          defaultValue: number
          /** 合法下限；缺省为 defaultValue */
          min?: number
          /** 合法上限（超过则截断） */
          max?: number
      }
    | { key: string; name?: string; type: "boolean"; defaultValue: boolean }
    | {
          key: string
          name?: string
          aliases?: readonly string[]
          type: "array"
          values: readonly string[]
      }
    | {
          key: string
          name?: string
          type: "custom"
          parse: (get: GetParam, state: ParsedUrlState) => unknown
          build?: (
              value: unknown,
              options?: Record<string, unknown>,
          ) => string | undefined
      }

export type UrlStateCodec<TState extends object> = {
    parse(searchParams: SearchParamsLike): TState
    build(state: TState, options?: Record<string, unknown>): string
    buildParams(
        state: TState,
        options?: Record<string, unknown>,
    ): URLSearchParams
}

function readParam(spec: UrlFieldSpec, get: GetParam): string | null {
    if (!("aliases" in spec)) return get(spec.key)
    const primary = get(spec.key)
    if (primary != null) return primary
    for (const alias of spec.aliases ?? []) {
        const value = get(alias)
        if (value != null) return value
    }
    return null
}

function fieldName(spec: UrlFieldSpec): string {
    return spec.name ?? spec.key
}

function parseField(
    spec: UrlFieldSpec,
    get: GetParam,
    state: ParsedUrlState,
): unknown {
    switch (spec.type) {
        case "string":
            return readParam(spec, get) ?? undefined
        case "enum": {
            const raw = readParam(spec, get)
            const candidate =
                raw == null
                    ? spec.defaultValue
                    : spec.normalize
                      ? spec.normalize(raw)
                      : raw
            if (candidate != null && spec.values.includes(candidate)) {
                return candidate
            }
            return spec.defaultValue
        }
        case "number": {
            const raw = readParam(spec, get)
            const parsed = Number(raw ?? String(spec.defaultValue))
            const min = spec.min ?? spec.defaultValue
            if (!Number.isFinite(parsed) || parsed < min)
                return spec.defaultValue
            let floored = Math.floor(parsed)
            if (spec.max != null) floored = Math.min(spec.max, floored)
            return floored
        }
        case "boolean": {
            const raw = readParam(spec, get)
            if (raw === "0") return false
            if (raw === "1") return true
            return spec.defaultValue
        }
        case "array": {
            const raw = readParam(spec, get)
            if (!raw) return undefined
            const values = raw
                .split(",")
                .map((s) => s.trim())
                .filter((s) => spec.values.includes(s))
            return values.length > 0 ? values : undefined
        }
        case "custom":
            return spec.parse(get, state)
    }
}

function defaultBuildWhen(spec: UrlFieldSpec): (value: unknown) => boolean {
    switch (spec.type) {
        case "string":
            return (value) =>
                Boolean(value) && (!spec.trim || Boolean(String(value).trim()))
        case "enum":
            return (value) => value !== undefined && value !== spec.defaultValue
        case "number":
            return (value) =>
                typeof value === "number" && value !== spec.defaultValue
        case "boolean":
            return (value) => value !== undefined
        case "array":
            return (value) => Array.isArray(value) && value.length > 0
        case "custom":
            return () => false
    }
}

function buildValue(spec: UrlFieldSpec, value: unknown): string {
    switch (spec.type) {
        case "string":
            return spec.trim ? String(value).trim() : String(value)
        case "enum":
        case "number":
            return String(value)
        case "boolean":
            return value ? "1" : "0"
        case "array":
            return (value as string[]).join(",")
        case "custom":
            return String(value)
    }
}

export function createUrlStateCodec<TState extends object>(
    fields: readonly UrlFieldSpec[],
): UrlStateCodec<TState> {
    const parse = (searchParams: SearchParamsLike): TState => {
        const get: GetParam = (name) => searchParams.get(name)
        const state: ParsedUrlState = {}
        for (const spec of fields) {
            state[fieldName(spec)] = parseField(spec, get, state)
        }
        return state as TState
    }

    const buildParams = (
        state: TState,
        options?: Record<string, unknown>,
    ): URLSearchParams => {
        const params = new URLSearchParams()
        const record = state as ParsedUrlState
        for (const spec of fields) {
            const value = record[fieldName(spec)]
            if (spec.type === "custom") {
                const out = spec.build?.(value, options)
                if (out != null) params.set(spec.key, out)
                continue
            }
            const shouldWrite =
                "buildWhen" in spec && spec.buildWhen
                    ? spec.buildWhen(value, record)
                    : defaultBuildWhen(spec)(value)
            if (!shouldWrite) continue
            params.set(spec.key, buildValue(spec, value))
        }
        return params
    }

    const build = (
        state: TState,
        options?: Record<string, unknown>,
    ): string => {
        const qs = buildParams(state, options).toString()
        return qs ? `?${qs}` : ""
    }

    return { parse, build, buildParams }
}
