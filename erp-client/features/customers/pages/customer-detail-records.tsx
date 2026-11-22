export function periodLabel(from: string, to?: string) {
    return to ? `${from} ~ ${to}` : `${from} 起`
}
