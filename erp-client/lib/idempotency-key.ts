/**
 * 生成带业务前缀的幂等键。
 *
 * 前缀只用于排查请求来源，服务端按整串去重；同一业务动作重试时应复用同一键。
 * `randomLength` 保留各调用方原有的随机段长度。
 */
export function newIdempotencyKey(prefix: string, randomLength = 6): string {
    return `${prefix}-${Date.now().toString(36)}-${Math.random()
        .toString(36)
        .slice(2, 2 + randomLength)}`
}
