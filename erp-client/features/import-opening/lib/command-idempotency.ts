/** 返回同一命令载荷在当前页面生命周期内复用的幂等键。 */
export function commandIdempotencyKey(
    keys: Map<string, string>,
    identity: string,
): string {
    const existing = keys.get(identity)
    if (existing) return existing
    const key = `w18:${crypto.randomUUID()}`
    keys.set(identity, key)
    return key
}
