/**
 * 页面动作的请求标识构造。
 * 界面禁止出现「幂等键」等实现词，此模块只服务于请求字段。
 */

export function createIdempotencyKey(
    operationId: string,
    documentVersion: number,
    action: "save" | "post",
): string {
    return `w09:${operationId}:${documentVersion}:${action}:${crypto.randomUUID()}`
}
