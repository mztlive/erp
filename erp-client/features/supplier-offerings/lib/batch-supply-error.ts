import { getErrorMessage } from "@/lib/api/errors"

/** 由批量文件解析和目录匹配生成、可直接展示的业务校验说明。 */
export class SupplyBatchValidationError extends Error {
    constructor(message: string) {
        super(message)
        this.name = "SupplyBatchValidationError"
    }
}

/** 保留明确的本地校验说明；网络和底层读取错误仍按统一规则处理。 */
export function batchValidationMessage(
    error: unknown,
    fallback: string,
): string {
    return error instanceof SupplyBatchValidationError
        ? error.message
        : getErrorMessage(error, fallback)
}
