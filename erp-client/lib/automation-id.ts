export function toAutomationIdSegment(value: unknown): string {
    let normalized: string

    try {
        normalized = String(value ?? "")
            .normalize("NFKD")
            .toLowerCase()
            .replace(/[\u0300-\u036f]/g, "")
    } catch {
        return "item"
    }

    return (
        normalized
            .replace(/[^a-z0-9]+/g, "-")
            .replace(/^-+|-+$/g, "") || "item"
    )
}
