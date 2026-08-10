export function formatEffectiveRange(from: string, to?: string): string {
    if (!to) return `${from} ~ 长期`
    return `${from} ~ ${to}`
}
