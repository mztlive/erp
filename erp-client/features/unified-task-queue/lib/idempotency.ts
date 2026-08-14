export const IDEMPOTENCY_PREFIX = "work-item-responsibility"

export function createIdempotencyKey(
    workItemId: string,
    action: string,
): string {
    return `${IDEMPOTENCY_PREFIX}:${workItemId}:${action}:${crypto.randomUUID()}`
}
