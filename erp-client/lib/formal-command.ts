import type { ApiError } from "./api/errors"

export type FormalCommandSettlement = "succeeded" | "failed" | "unknown"

export type FormalCommandIdentity<T> = Readonly<{
    idempotencyKey: string
    payload: T
}>

type KeyFactory = (prefix: string) => string

const isApiError = (error: unknown): error is ApiError =>
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    "message" in error

/**
 * 网络中断或成功响应无法解析时，客户端无法证明服务端没有执行命令。
 * HTTP 4xx 等已收到的业务拒绝属于确定失败，可以开始一笔新尝试。
 */
export const classifyFormalCommandError = (
    error: unknown,
): Exclude<FormalCommandSettlement, "succeeded"> =>
    isApiError(error) &&
    (error.kind === "Network" ||
        error.kind === "Parse" ||
        (error.status != null && error.status >= 500))
        ? "unknown"
        : "failed"

const clonePayload = <T>(payload: T): T => structuredClone(payload)

const defaultKeyFactory: KeyFactory = (prefix) => {
    const randomId = globalThis.crypto?.randomUUID?.()
    if (!randomId) {
        throw new Error("当前环境无法生成安全的操作标识")
    }
    return `${prefix}:${randomId}`
}

/**
 * 页面生命周期内保存正式命令身份。
 *
 * 同一动作在结果未知时返回最初的载荷和幂等键；只有确认成功或确定失败才清除。
 * 账本不持久化，避免不同业务对象或后续会话错误复用同一个命令身份。
 */
export class FormalCommandKeyLedger {
    readonly #entries = new Map<string, FormalCommandIdentity<unknown>>()
    readonly #keyFactory: KeyFactory

    constructor(keyFactory: KeyFactory = defaultKeyFactory) {
        this.#keyFactory = keyFactory
    }

    acquire<T>(
        slot: string,
        prefix: string,
        payload: T,
    ): FormalCommandIdentity<T> {
        const current = this.#entries.get(slot)
        if (current) return current as FormalCommandIdentity<T>

        const identity: FormalCommandIdentity<T> = {
            idempotencyKey: this.#keyFactory(prefix),
            payload: clonePayload(payload),
        }
        this.#entries.set(slot, identity)
        return identity
    }

    peek<T>(slot: string): FormalCommandIdentity<T> | undefined {
        return this.#entries.get(slot) as FormalCommandIdentity<T> | undefined
    }

    settle(slot: string, settlement: FormalCommandSettlement): void {
        if (settlement !== "unknown") this.#entries.delete(slot)
    }
}
