/** 变更命令共享守卫与错误映射。 */

import {
    WAREHOUSE_WRITE_CODE,
    WAREHOUSE_WRITE_MESSAGE,
} from "@/features/master-data/lib/data"
import { isApiError } from "@/features/master-data/api/presentation"
import type {
    MasterDataMutationResult,
    ProductKind,
} from "@/features/master-data/types"

export function blockedWarehouse(): MasterDataMutationResult {
    return {
        outcome: "blocked",
        code: WAREHOUSE_WRITE_CODE,
        message: WAREHOUSE_WRITE_MESSAGE,
        detail: "仓库资料暂不可维护，任何角色都不能改。",
    }
}

export function mapMutationError(
    error: unknown,
    fallbackLock?: { version: number; revisionNo: number },
): MasterDataMutationResult {
    if (!isApiError(error)) {
        throw error
    }
    if (error.status === 409) {
        return {
            outcome: "conflict",
            message: "资料已被他人更新，请刷新后重新填写。",
            serverLockVersion: fallbackLock?.version ?? 0,
            serverRevisionNo: fallbackLock?.revisionNo ?? 0,
        }
    }
    if (
        error.kind === "Validation" ||
        error.status === 400 ||
        error.status === 422
    ) {
        return {
            outcome: "blocked",
            code: "VALIDATION",
            message: error.message || "请求未通过业务校验",
        }
    }
    // Let network/auth/5xx propagate for Query error state
    throw error
}

export function mapProductKindInput(kind: string | undefined): ProductKind {
    if (
        kind === "PHYSICAL" ||
        kind === "VIRTUAL" ||
        kind === "OFFLINE_SERVICE" ||
        kind === "VOUCHER"
    ) {
        return kind
    }
    // Chinese labels from category form
    switch (kind) {
        case "实物":
            return "PHYSICAL"
        case "虚拟":
            return "VIRTUAL"
        case "服务":
        case "线下服务":
            return "OFFLINE_SERVICE"
        case "卡券":
            return "VOUCHER"
        default:
            return "PHYSICAL"
    }
}

export function parseQuantityScale(raw: string | undefined): number | null {
    const value = Number((raw ?? "").trim())
    if (!Number.isInteger(value) || value < 0 || value > 6) return null
    return value
}
