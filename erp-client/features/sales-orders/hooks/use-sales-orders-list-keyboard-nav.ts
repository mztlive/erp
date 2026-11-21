import * as React from "react"

export function useSalesOrdersListKeyboardNav(enabled: boolean) {
    React.useEffect(() => {
        if (!enabled) return

        const onKeyDown = (event: KeyboardEvent) => {
            if (
                event.defaultPrevented ||
                event.isComposing ||
                event.altKey ||
                event.ctrlKey ||
                event.metaKey ||
                (event.key !== "j" && event.key !== "k") ||
                !(event.target instanceof HTMLTableRowElement) ||
                !event.target.matches(
                    "#sales-orders-list-table tr[data-row-id]",
                )
            ) {
                return
            }
            const row =
                event.key === "j"
                    ? event.target.nextElementSibling
                    : event.target.previousElementSibling
            if (
                row instanceof HTMLTableRowElement &&
                row.hasAttribute("data-row-id")
            ) {
                event.preventDefault()
                row.focus()
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [enabled])
}
